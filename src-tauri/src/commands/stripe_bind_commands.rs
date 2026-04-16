use crate::models::{OperationLog, OperationType, OperationStatus};
use crate::repository::DataStore;
use crate::services::WindsurfService;
use crate::utils::AppError;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, atomic::{AtomicU32, Ordering}};
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;
use tokio::sync::Mutex;
use once_cell::sync::Lazy;
use rand::Rng;
use base64::{Engine as _, engine::general_purpose};
use reqwest::header::{HeaderMap, HeaderValue};
use std::ffi::OsStr;

// ─── 全局任务状态管理 ───────────────────────────────────────────
static BIND_TASKS: Lazy<Mutex<HashMap<String, BindTaskState>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindTaskState {
    pub task_id: String,
    pub account_id: String,
    pub email: String,
    pub status: String, // pending | running | success | failed | cancelled
    pub step: i32,      // 0-6
    pub step_name: String,
    pub error: Option<String>,
    pub stripe_url: Option<String>,
}

// ─── 输入参数结构 ─────────────────────────────────────────────
#[derive(Debug, Clone, Deserialize)]
pub struct CardInfo {
    pub number: String,
    pub cvc: String,
    pub exp_year: String,
    pub exp_month: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressInfo {
    pub country: String,
    pub line1: String,
    pub city: String,
    pub state: String,
    pub postal_code: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CaptchaConfig {
    pub api_url: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProxyConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub pass: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StripeBindRequest {
    pub account_ids: Vec<String>,
    pub cards: Vec<CardInfo>,
    pub captcha: CaptchaConfig,
    pub proxy: Option<ProxyConfig>,
    pub teams_tier: Option<i32>,
    pub payment_period: Option<i32>,
    pub concurrency: Option<usize>,
    pub custom_address: Option<AddressInfo>,
    pub custom_name: Option<String>,
    pub turnstile_tokens: Option<std::collections::HashMap<String, String>>,
    pub debug_cards: Option<Vec<CardInfo>>,
    pub max_debug_failures: Option<u32>,
    pub presolve_captcha: Option<bool>,
}

// ─── PX 令牌结构 ────────────────────────────────────────────
#[derive(Debug, Clone, Default)]
struct PxTokens {
    px3: String,
    pxvid: String,
    pxcts: String,
    js_checksum: String,
    rv_timestamp: String,
    stripe_version_hash: String,
}

// ─── 常量 ───────────────────────────────────────────────────
const STRIPE_API: &str = "https://api.stripe.com";
const STRIPE_VERSION_FULL: &str = "2025-03-31.basil; checkout_server_update_beta=v1; checkout_manual_approval_preview=v1";
const STRIPE_VERSION_BASE: &str = "2025-03-31.basil";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36";
const WINDSURF_HOST: &str = "checkout.stripe.com";
const WINDSURF_ORIGIN: &str = "https://checkout.stripe.com";
const KNOWN_PK: &str = "pk_live_51NRMxXFKuRRGjKOF8UiLeVezJmJe3xlk8tHCRctncoDJmMElhArAYMgN1n5s3tOMdlDyJZZkm1KcEa386dj5XS8d00TmPn497w";
const HCAPTCHA_SITE_KEY_FALLBACK: &str = "ec637546-e9b8-447a-ab81-b5fb6d228ab8";

// ─── 美国地址数据库 ──────────────────────────────────────────
const US_FIRST_NAMES: &[&str] = &[
    "JAMES", "JOHN", "ROBERT", "MICHAEL", "WILLIAM", "DAVID", "RICHARD", "JOSEPH",
    "THOMAS", "CHARLES", "DANIEL", "MATTHEW", "ANTHONY", "MARK", "STEVEN",
    "MARY", "PATRICIA", "JENNIFER", "LINDA", "ELIZABETH", "BARBARA", "SUSAN",
    "JESSICA", "SARAH", "KAREN", "NANCY", "LISA", "BETTY", "MARGARET", "SANDRA",
];
const US_LAST_NAMES: &[&str] = &[
    "SMITH", "JOHNSON", "WILLIAMS", "BROWN", "JONES", "GARCIA", "MILLER",
    "DAVIS", "RODRIGUEZ", "MARTINEZ", "WILSON", "ANDERSON", "TAYLOR", "THOMAS",
    "MOORE", "JACKSON", "MARTIN", "LEE", "THOMPSON", "WHITE", "HARRIS", "CLARK",
];
const US_STREETS: &[&str] = &[
    "Main St", "Oak Ave", "Elm St", "Pine Rd", "Maple Dr", "Cedar Ln",
    "Washington Blvd", "Park Ave", "Lake St", "Hill Rd", "River Dr", "Forest Ave",
    "Sunset Blvd", "Broadway", "Church St", "Spring St", "Center St", "Highland Ave",
];
const US_CITIES_STATES: &[(&str, &str, &str)] = &[
    ("New York", "NY", "10001"), ("Los Angeles", "CA", "90001"),
    ("Chicago", "IL", "60601"), ("Houston", "TX", "77001"),
    ("Phoenix", "AZ", "85001"), ("Philadelphia", "PA", "19101"),
    ("San Antonio", "TX", "78201"), ("San Diego", "CA", "92101"),
    ("Dallas", "TX", "75201"), ("San Jose", "CA", "95101"),
    ("Austin", "TX", "78701"), ("Jacksonville", "FL", "32099"),
    ("Columbus", "OH", "43085"), ("Charlotte", "NC", "28201"),
    ("Indianapolis", "IN", "46201"), ("Denver", "CO", "80201"),
    ("Seattle", "WA", "98101"), ("Nashville", "TN", "37201"),
    ("Portland", "OR", "97201"), ("Las Vegas", "NV", "89101"),
    ("Atlanta", "GA", "30301"), ("Miami", "FL", "33101"),
    ("Minneapolis", "MN", "55401"), ("Tampa", "FL", "33601"),
];

fn generate_random_address() -> AddressInfo {
    let mut rng = rand::thread_rng();
    let (city, state, zip) = US_CITIES_STATES[rng.gen_range(0..US_CITIES_STATES.len())];
    let street_num = rng.gen_range(100..9999);
    let street = US_STREETS[rng.gen_range(0..US_STREETS.len())];
    AddressInfo {
        country: "US".to_string(),
        line1: format!("{} {}", street_num, street),
        city: city.to_string(),
        state: state.to_string(),
        postal_code: zip.to_string(),
    }
}

fn generate_random_name() -> String {
    let mut rng = rand::thread_rng();
    let first = US_FIRST_NAMES[rng.gen_range(0..US_FIRST_NAMES.len())];
    let last = US_LAST_NAMES[rng.gen_range(0..US_LAST_NAMES.len())];
    format!("{} {}", first, last)
}

fn generate_random_email() -> String {
    let mut rng = rand::thread_rng();
    let domains = ["gmail.com", "yahoo.com", "outlook.com", "hotmail.com", "icloud.com"];
    let len: usize = rng.gen_range(8..12);
    let user: String = (0..len)
        .map(|_| {
            let idx: u8 = rng.gen_range(0..36);
            if idx < 26 { (b'a' + idx) as char } else { (b'0' + (idx - 26)) as char }
        })
        .collect();
    format!("{}@{}", user, domains[rng.gen_range(0..domains.len())])
}

// ─── 获取当前 Stripe.js 版本哈希 ────────────────────────────────
async fn fetch_stripe_js_version() -> String {
    let url = "https://js.stripe.com/v3/.deploy_status_henson.json";
    if let Ok(resp) = reqwest::Client::new()
        .get(url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        if let Ok(data) = resp.json::<serde_json::Value>().await {
            if let Some(v) = data.get("version").and_then(|v| v.as_str()) {
                return v.to_string();
            }
        }
    }
    "d0116183d3".to_string() // fallback to known recent version
}

// ─── 通过无头浏览器获取 PerimeterX 令牌 ─────────────────────────
async fn fetch_px_tokens(
    checkout_url: &str,
    app: &AppHandle,
    task_id: &str,
) -> Result<PxTokens, String> {
    emit_log(app, task_id, "info", "  [PX] 正在启动浏览器获取反机器人令牌 ...");

    let url = checkout_url.to_string();

    let tokens = tokio::task::spawn_blocking(move || -> Result<PxTokens, String> {
        use headless_chrome::{Browser, LaunchOptions};

        let launch_options = LaunchOptions {
            headless: true,
            sandbox: false,
            window_size: Some((1920, 1080)),
            args: vec![
                OsStr::new("--disable-blink-features=AutomationControlled"),
                OsStr::new("--disable-features=IsolateOrigins,site-per-process"),
                OsStr::new("--disable-web-security"),
                OsStr::new("--no-first-run"),
                OsStr::new("--disable-extensions"),
            ],
            ..Default::default()
        };

        let browser = Browser::new(launch_options)
            .map_err(|e| format!("启动浏览器失败 (请确保已安装 Chrome/Edge): {}", e))?;

        let tab = browser.new_tab()
            .map_err(|e| format!("创建标签页失败: {}", e))?;

        // 隐藏 webdriver 特征
        let _ = tab.evaluate(
            r#"Object.defineProperty(navigator, 'webdriver', {get: () => undefined})"#,
            false,
        );

        tab.set_user_agent(USER_AGENT, None, None)
            .map_err(|e| format!("设置 UA 失败: {}", e))?;

        tab.navigate_to(&url)
            .map_err(|e| format!("导航失败: {}", e))?;

        // 等待页面基本加载
        let _ = tab.wait_for_element_with_custom_timeout("button", std::time::Duration::from_secs(15));

        // 等待 PX 脚本执行生成令牌
        std::thread::sleep(std::time::Duration::from_secs(8));

        // 提取 cookies
        let cookies = tab.get_cookies()
            .map_err(|e| format!("获取 cookies 失败: {}", e))?;

        let px3 = cookies.iter()
            .find(|c| c.name == "_px3")
            .map(|c| c.value.clone())
            .unwrap_or_default();

        let pxvid = cookies.iter()
            .find(|c| c.name == "_pxvid")
            .map(|c| c.value.clone())
            .unwrap_or_default();

        // pxcts 是一个 UUID 时间戳
        let pxcts = Uuid::new_v4().to_string();

        // 尝试提取 js_checksum (Stripe.js 内部生成)
        let js_checksum = tab.evaluate(
            r#"
            (function() {
                try {
                    // 尝试从 Stripe 内部状态获取
                    var scripts = document.querySelectorAll('script[src*="js.stripe.com"]');
                    for (var i = 0; i < scripts.length; i++) {
                        var m = scripts[i].src.match(/\/v3\/([a-f0-9]+)\//);
                        if (m) return m[1];
                    }
                } catch(e) {}
                return '';
            })()
            "#,
            false,
        ).ok()
            .and_then(|r| r.value)
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_default();

        // 提取 Stripe.js 版本哈希
        let stripe_version = tab.evaluate(
            r#"
            (function() {
                try {
                    var scripts = document.querySelectorAll('script[src]');
                    for (var i = 0; i < scripts.length; i++) {
                        var m = scripts[i].src.match(/js\.stripe\.com\/v3\/([a-f0-9]{8,})\//);
                        if (m) return m[1];
                    }
                } catch(e) {}
                return '';
            })()
            "#,
            false,
        ).ok()
            .and_then(|r| r.value)
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_default();

        Ok(PxTokens {
            px3,
            pxvid,
            pxcts,
            js_checksum,
            rv_timestamp: String::new(),
            stripe_version_hash: stripe_version,
        })
    }).await.map_err(|e| format!("浏览器任务异常: {}", e))??;

    if tokens.px3.is_empty() {
        emit_log(app, task_id, "warn", "  [PX] 未获取到 px3 令牌，confirm 可能被拦截");
    } else {
        emit_log(app, task_id, "info", &format!(
            "  [PX] 令牌获取成功: px3={}字符, pxvid={}",
            tokens.px3.len(),
            if tokens.pxvid.is_empty() { "(空)" } else { &tokens.pxvid[..tokens.pxvid.len().min(20)] }
        ));
    }

    Ok(tokens)
}

// ─── 日志发送辅助 ─────────────────────────────────────────────
fn emit_log(app: &AppHandle, task_id: &str, level: &str, msg: &str) {
    let ts = chrono::Local::now().format("%H:%M:%S%.3f").to_string();
    let _ = app.emit("stripe-bind-log", json!({
        "task_id": task_id,
        "level": level,
        "time": ts,
        "message": msg,
    }));
    println!("[StripeBind][{}] [{}] {}", task_id, level, msg);
}

fn emit_progress(app: &AppHandle, task_id: &str, account_id: &str, step: i32, step_name: &str, status: &str) {
    let _ = app.emit("stripe-bind-progress", json!({
        "task_id": task_id,
        "account_id": account_id,
        "step": step,
        "step_name": step_name,
        "status": status,
    }));
}

// ─── 构建 HTTP 客户端 ─────────────────────────────────────────
fn build_stripe_client(proxy: &Option<ProxyConfig>) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(15))
        .pool_max_idle_per_host(2)
        .pool_idle_timeout(std::time::Duration::from_secs(30))
        .http1_only()
        .user_agent(USER_AGENT);

    if let Some(p) = proxy {
        if let Some(host) = &p.host {
            if !host.is_empty() {
                let port = p.port.unwrap_or(10808);
                let proxy_url = if let (Some(user), Some(pass)) = (&p.user, &p.pass) {
                    if !user.is_empty() {
                        format!("http://{}:{}@{}:{}", user, pass, host, port)
                    } else {
                        format!("http://{}:{}", host, port)
                    }
                } else {
                    format!("http://{}:{}", host, port)
                };
                builder = builder.proxy(
                    reqwest::Proxy::all(&proxy_url).map_err(|e| format!("Invalid proxy: {}", e))?
                );
            }
        }
    }

    builder.build().map_err(|e| format!("Failed to build HTTP client: {}", e))
}

fn stripe_headers() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert("Accept", HeaderValue::from_static("application/json"));
    h.insert("Origin", HeaderValue::from_static("https://js.stripe.com"));
    h.insert("Referer", HeaderValue::from_static("https://js.stripe.com/"));
    h
}

// ─── 解析 checkout session_id ─────────────────────────────────
fn parse_session_id(raw: &str) -> Result<String, String> {
    let re = regex::Regex::new(r"(cs_(?:live|test)_[A-Za-z0-9]+)").unwrap();
    if let Some(cap) = re.captures(raw) {
        Ok(cap[1].to_string())
    } else {
        Err(format!("无法从输入中提取 checkout_session_id: {}", &raw[..raw.len().min(120)]))
    }
}

// ─── 指纹注册 (m.stripe.com/6) ────────────────────────────────
async fn register_fingerprint(
    client: &reqwest::Client,
    app: &AppHandle,
    task_id: &str,
) -> Result<(String, String, String), String> {
    let guid = Uuid::new_v4().to_string().replace("-", "") + &Uuid::new_v4().to_string()[..6];
    let muid_init = Uuid::new_v4().to_string().replace("-", "") + &Uuid::new_v4().to_string()[..6];
    let sid_init = Uuid::new_v4().to_string().replace("-", "") + &Uuid::new_v4().to_string()[..6];

    let mut muid = muid_init;
    let mut guid_final = guid.clone();
    let sid = sid_init;

    emit_log(app, task_id, "info", "  [指纹] 向 m.stripe.com/6 注册设备指纹 ...");

    let m6_url = "https://m.stripe.com/6";
    let fp_id = Uuid::new_v4().to_string().replace("-", "");

    // Pre-compute all random values before any await point (ThreadRng is !Send)
    let (rand_t, rand_hex) = {
        let mut rng = rand::thread_rng();
        let t = rng.gen_range(3.0..120.0_f64);
        let h: [u8; 10] = rng.gen();
        (t, hex::encode(&h))
    };

    // 构建指纹 payload
    let payload = json!({
        "v2": 1,
        "id": fp_id,
        "t": rand_t,
        "tag": "$npm_package_version",
        "src": "js",
        "a": {
            "a": {"v": "true", "t": 0},
            "b": {"v": "true", "t": 0},
            "c": {"v": "en-US", "t": 0},
            "d": {"v": "Win32", "t": 0},
            "f": {"v": "1920w_1040h_24d_1r", "t": 0},
            "g": {"v": "8", "t": 0},
            "h": {"v": "false", "t": 0},
            "l": {"v": USER_AGENT, "t": 0},
        },
        "b": {
            "d": "NA",
            "e": "NA",
            "f": false,
            "g": true,
            "h": true,
            "i": ["location"],
            "j": [],
            "u": WINDSURF_HOST,
            "v": WINDSURF_HOST,
        },
        "h": rand_hex,
    });

    let raw_json = serde_json::to_string(&payload).unwrap_or_default();
    let encoded = general_purpose::STANDARD.encode(
        urlencoding::encode(&raw_json).as_bytes()
    );

    let resp = client.post(m6_url)
        .header("Content-Type", "text/plain;charset=UTF-8")
        .header("Accept", "*/*")
        .header("Origin", "https://m.stripe.network")
        .header("Referer", "https://m.stripe.network/")
        .body(encoded)
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            if let Ok(j) = r.json::<serde_json::Value>().await {
                if let Some(m) = j.get("muid").and_then(|v| v.as_str()) {
                    muid = m.to_string();
                }
                if let Some(g) = j.get("guid").and_then(|v| v.as_str()) {
                    guid_final = g.to_string();
                }
            }
            emit_log(app, task_id, "info", &format!("  [指纹] OK → muid={}...", &muid[..muid.len().min(20)]));
        }
        Ok(r) => {
            emit_log(app, task_id, "warn", &format!("  [指纹] 返回 {}", r.status()));
        }
        Err(e) => {
            emit_log(app, task_id, "warn", &format!("  [指纹] 失败: {}", e));
        }
    }

    Ok((guid_final, muid, sid))
}

// ─── 初始化 Checkout Session ──────────────────────────────────
async fn init_checkout(
    client: &reqwest::Client,
    session_id: &str,
    app: &AppHandle,
    task_id: &str,
) -> Result<(serde_json::Value, String), String> {
    let url = format!("{}/v1/payment_pages/{}/init", STRIPE_API, session_id);

    for version in [STRIPE_VERSION_BASE, STRIPE_VERSION_FULL] {
        let mut params = HashMap::new();
        params.insert("key", KNOWN_PK);
        params.insert("eid", "NA");
        params.insert("browser_locale", "en-US");
        params.insert("browser_timezone", "America/Chicago");
        params.insert("redirect_type", "url");

        emit_log(app, task_id, "info", &format!("  初始化结账会话 version={}...", &version[..version.len().min(30)]));

        let resp = client.post(&url)
            .headers(stripe_headers())
            .form(&params)
            .send()
            .await
            .map_err(|e| format!("init 请求失败: {}", e))?;

        let status = resp.status();
        if status.is_success() {
            let body: serde_json::Value = resp.json().await
                .map_err(|e| format!("init 解析失败: {}", e))?;
            emit_log(app, task_id, "info", "  init 成功");
            // 打印 init 响应关键字段
            let top_keys: Vec<&str> = body.as_object().map(|m| m.keys().map(|k| k.as_str()).collect()).unwrap_or_default();
            emit_log(app, task_id, "debug", &format!("  [init] 顶层字段: {:?}", &top_keys[..top_keys.len().min(20)]));
            let ic = body.get("init_checksum").and_then(|v| v.as_str()).unwrap_or("(无)");
            let cid = body.get("config_id").and_then(|v| v.as_str()).unwrap_or("(无)");
            let merchant = body.get("merchant_name").and_then(|v| v.as_str()).unwrap_or("(无)");
            emit_log(app, task_id, "debug", &format!("  [init] init_checksum={}, config_id={}", &ic[..ic.len().min(30)], &cid[..cid.len().min(30)]));
            // 更多关键字段
            let amount = body.get("amount").or(body.get("total")).and_then(|v| v.as_i64());
            let currency = body.get("currency").and_then(|v| v.as_str()).unwrap_or("(无)");
            let pm_types = body.get("automatic_payment_method_types").and_then(|v| serde_json::to_string(v).ok()).unwrap_or_default();
            let has_blob = body.get("blob").is_some();
            let mode = body.get("mode").and_then(|v| v.as_str()).unwrap_or("(无)");
            emit_log(app, task_id, "debug", &format!(
                "  [init] amount={:?}, currency={}, mode={}, has_blob={}, pm_types={}",
                amount, currency, mode, has_blob, &pm_types[..pm_types.len().min(100)]
            ));
            emit_log(app, task_id, "info", &format!("  商户: {}", merchant));
            return Ok((body, version.to_string()));
        }

        let body_text = resp.text().await.unwrap_or_default();
        if status.as_u16() == 400 && body_text.to_lowercase().contains("beta") {
            emit_log(app, task_id, "warn", "  版本不支持 beta, 尝试下一个...");
            continue;
        }
        return Err(format!("init 失败 [{}]: {}", status, &body_text[..body_text.len().min(500)]));
    }

    Err("init 失败: 所有 Stripe API 版本均不可用".to_string())
}

// ─── 提取 hCaptcha 配置 ───────────────────────────────────────
fn extract_hcaptcha_config(init_resp: &serde_json::Value) -> (String, String) {
    let raw = serde_json::to_string(init_resp).unwrap_or_default();
    let mut site_key = HCAPTCHA_SITE_KEY_FALLBACK.to_string();
    let mut rqdata = String::new();

    if let Some(sk) = init_resp.get("site_key").and_then(|v| v.as_str()) {
        site_key = sk.to_string();
    } else {
        let re = regex::Regex::new(r#""hcaptcha_site_key"\s*:\s*"([^"]+)""#).unwrap();
        if let Some(cap) = re.captures(&raw) {
            site_key = cap[1].to_string();
        }
    }

    let re_rq = regex::Regex::new(r#""hcaptcha_rqdata"\s*:\s*"([^"]+)""#).unwrap();
    if let Some(cap) = re_rq.captures(&raw) {
        rqdata = cap[1].to_string();
    }

    (site_key, rqdata)
}

// ─── 获取 elements session (Python 的 fetch_elements_session) ───
async fn fetch_elements_session(
    client: &reqwest::Client,
    session_id: &str,
    stripe_js_id: &str,
    stripe_ver: &str,
    app: &AppHandle,
    task_id: &str,
) -> Result<(String, String), String> {
    let url = format!("{}/v1/elements/sessions", STRIPE_API);
    let params = [
        ("client_betas[0]", "google_pay_beta_1"),
        ("client_betas[1]", "disable_deferred_intent_client_validation_beta_1"),
        ("client_betas[2]", "blocked_card_brands_beta_2"),
        ("deferred_intent[mode]", "subscription"),
        ("deferred_intent[amount]", "0"),
        ("deferred_intent[currency]", "usd"),
        ("deferred_intent[setup_future_usage]", "off_session"),
        ("deferred_intent[payment_method_types][0]", "card"),
        ("deferred_intent[payment_method_types][1]", "link"),
        ("currency", "usd"),
        ("key", KNOWN_PK),
        ("_stripe_version", stripe_ver),
        ("elements_init_source", "checkout"),
        ("hosted_surface", "checkout"),
        ("referrer_host", WINDSURF_HOST),
        ("stripe_js_id", stripe_js_id),
        ("locale", "en"),
        ("type", "deferred_intent"),
        ("checkout_session_id", session_id),
    ];

    emit_log(app, task_id, "info", "[2c/6] 获取 elements session ...");
    let resp = client.get(&url)
        .headers(stripe_headers())
        .query(&params)
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            let data: serde_json::Value = r.json().await.unwrap_or_default();
            let es_id = data.get("session_id")
                .or(data.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let config_id = data.get("config_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if !es_id.is_empty() {
                emit_log(app, task_id, "debug", &format!("  [elements] session_id: {}", &es_id[..es_id.len().min(30)]));
            }
            if !config_id.is_empty() {
                emit_log(app, task_id, "debug", &format!("  [elements] config_id: {}", &config_id[..config_id.len().min(30)]));
            }
            Ok((es_id, config_id))
        }
        Ok(r) => {
            let status = r.status();
            emit_log(app, task_id, "warn", &format!("  [elements] 返回 {}, 继续使用本地生成的 ID", status));
            Ok((String::new(), String::new()))
        }
        Err(e) => {
            emit_log(app, task_id, "warn", &format!("  [elements] 请求失败: {}, 继续", e));
            Ok((String::new(), String::new()))
        }
    }
}

// ─── 逐字段提交地址 ──────────────────────────────────────────
async fn update_address(
    client: &reqwest::Client,
    session_id: &str,
    addr: &AddressInfo,
    _stripe_js_id: &str,
    _elements_session_id: &str,
    _stripe_ver: &str,
    app: &AppHandle,
    task_id: &str,
) -> Result<(), String> {
    let url = format!("{}/v1/payment_pages/{}", STRIPE_API, session_id);

    let steps: Vec<Vec<(&str, &str)>> = vec![
        vec![("tax_region[country]", &addr.country)],
        vec![],
        vec![("tax_region[line1]", &addr.line1)],
        vec![("tax_region[city]", &addr.city)],
        vec![("tax_region[state]", &addr.state)],
        vec![("tax_region[postal_code]", &addr.postal_code)],
    ];

    emit_log(app, task_id, "info", "  [address] 逐字段提交税区地址 ...");

    let mut accumulated: Vec<(String, String)> = vec![];
    for (i, step_fields) in steps.iter().enumerate() {
        for (k, v) in step_fields {
            accumulated.push((k.to_string(), v.to_string()));
        }

        let mut form_data: Vec<(String, String)> = vec![
            ("eid".to_string(), "NA".to_string()),
        ];
        form_data.extend(accumulated.clone());
        form_data.push(("key".to_string(), KNOWN_PK.to_string()));

        let step_name = if step_fields.is_empty() { "(焦点变更)" } else { step_fields[0].0 };
        emit_log(app, task_id, "debug", &format!("  [address] step {}/6: {}", i + 1, step_name));

        let resp = client.post(&url)
            .headers(stripe_headers())
            .form(&form_data)
            .send()
            .await;

        match resp {
            Ok(r) if !r.status().is_success() => {
                let status = r.status();
                let body = r.text().await.unwrap_or_default();
                emit_log(app, task_id, "warn", &format!("  [address] step {} 返回 {}: {}", i + 1, status, &body[..body.len().min(200)]));
            }
            Err(e) => {
                emit_log(app, task_id, "warn", &format!("  [address] step {} 请求失败: {}", i + 1, e));
            }
            _ => {}
        }

        let delay: u64 = { rand::thread_rng().gen_range(1500..3500) };
        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
    }

    Ok(())
}

// ─── 解 hCaptcha ──────────────────────────────────────────────
async fn solve_hcaptcha(
    captcha_cfg: &CaptchaConfig,
    site_key: &str,
    app: &AppHandle,
    task_id: &str,
) -> Result<(String, String), String> {
    let max_retries = 3;

    for retry in 0..max_retries {
        if retry > 0 {
            emit_log(app, task_id, "info", &format!("  --- 重试第 {}/{} 次 ---", retry + 1, max_retries));
        }

        emit_log(app, task_id, "info", &format!("  解 hCaptcha (siteKey: {}...)", &site_key[..site_key.len().min(20)]));

        let task_body = json!({
            "type": "HCaptchaTaskProxyless",
            "websiteURL": "https://b.stripecdn.com/stripethirdparty-srv/assets/v32.1/HCaptchaInvisible.html",
            "websiteKey": site_key,
            "isEnterprise": true,
            "userAgent": USER_AGENT,
        });

        let create_url = format!("{}/createTask", captcha_cfg.api_url.trim_end_matches('/'));
        let create_payload = json!({
            "clientKey": captcha_cfg.api_key,
            "task": task_body,
        });

        emit_log(app, task_id, "debug", &format!("  [captcha] URL: {}", create_url));
        emit_log(app, task_id, "debug", &format!("  [captcha] key: {}...{}", &captcha_cfg.api_key[..captcha_cfg.api_key.len().min(8)], &captcha_cfg.api_key[captcha_cfg.api_key.len().saturating_sub(6)..]));

        let http = reqwest::Client::new();
        let create_resp = http.post(&create_url)
            .json(&create_payload)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| format!("创建验证码任务失败: {}", e))?;

        let data: serde_json::Value = create_resp.json().await
            .map_err(|e| format!("解析验证码响应失败: {}", e))?;

        if data.get("errorId").and_then(|v| v.as_i64()).unwrap_or(1) != 0 {
            let desc = data.get("errorDescription").and_then(|v| v.as_str()).unwrap_or("?");
            let err_code = data.get("errorCode").and_then(|v| v.as_str()).unwrap_or("?");
            emit_log(app, task_id, "error", &format!("  任务创建失败: {} (code: {}, full: {})", desc, err_code, data));
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            continue;
        }

        let captcha_task_id = data.get("taskId").and_then(|v| v.as_str()).unwrap_or("");
        emit_log(app, task_id, "info", &format!("  任务: {} 等待解题 ...", captcha_task_id));

        let result_url = format!("{}/getTaskResult", captcha_cfg.api_url);
        for attempt in 0..60 {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;

            let result_payload = json!({
                "clientKey": captcha_cfg.api_key,
                "taskId": captcha_task_id,
            });

            let result_resp = match http.post(&result_url)
                .json(&result_payload)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
            {
                Ok(r) => r,
                Err(_) => continue,
            };

            let result_data: serde_json::Value = match result_resp.json().await {
                Ok(d) => d,
                Err(_) => continue,
            };

            if result_data.get("errorId").and_then(|v| v.as_i64()).unwrap_or(0) != 0 {
                let err_code = result_data.get("errorCode").and_then(|v| v.as_str()).unwrap_or("");
                if err_code == "ERROR_TASK_TIMEOUT" {
                    emit_log(app, task_id, "warn", "  任务超时, 重新发起...");
                    break;
                }
                continue;
            }

            if result_data.get("status").and_then(|v| v.as_str()) == Some("ready") {
                let solution = &result_data["solution"];
                let token = solution.get("gRecaptchaResponse")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let ekey = solution.get("eKey")
                    .or(solution.get("respKey"))
                    .or(solution.get("ekey"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                emit_log(app, task_id, "info", &format!("  已解决 (token: {} chars)", token.len()));
                return Ok((token, ekey));
            }

            if attempt % 5 == 4 {
                emit_log(app, task_id, "info", &format!("  等待中 ... ({}/60)", attempt + 1));
            }
        }
    }

    Err(format!("验证码解题失败 (已重试 {} 轮)", max_retries))
}

// ─── 创建 Payment Method ─────────────────────────────────────
async fn create_payment_method(
    client: &reqwest::Client,
    card: &CardInfo,
    addr: &AddressInfo,
    name: &str,
    email: &str,
    captcha_token: &str,
    session_id: &str,
    config_id: &str,
    stripe_js_id: &str,
    guid: &str,
    muid: &str,
    sid: &str,
    app: &AppHandle,
    task_id: &str,
) -> Result<String, String> {
    let mut data: Vec<(String, String)> = vec![
        ("type".into(), "card".into()),
        ("card[number]".into(), card.number.clone()),
        ("card[cvc]".into(), card.cvc.clone()),
        ("card[exp_month]".into(), card.exp_month.clone()),
        ("card[exp_year]".into(), card.exp_year.clone()),
        ("billing_details[name]".into(), name.to_string()),
        ("billing_details[email]".into(), email.to_string()),
        ("billing_details[address][country]".into(), addr.country.clone()),
        ("billing_details[address][line1]".into(), addr.line1.clone()),
        ("billing_details[address][city]".into(), addr.city.clone()),
        ("billing_details[address][postal_code]".into(), addr.postal_code.clone()),
        ("billing_details[address][state]".into(), addr.state.clone()),
        ("guid".into(), guid.to_string()),
        ("muid".into(), muid.to_string()),
        ("sid".into(), sid.to_string()),
        ("key".into(), KNOWN_PK.into()),
        ("payment_user_agent".into(), format!("stripe.js/{}; stripe-js-v3/{}; checkout", "d0116183d3", "d0116183d3")),
        ("client_attribution_metadata[client_session_id]".into(), stripe_js_id.to_string()),
        ("client_attribution_metadata[checkout_session_id]".into(), session_id.to_string()),
        ("client_attribution_metadata[merchant_integration_source]".into(), "checkout".into()),
        ("client_attribution_metadata[merchant_integration_version]".into(), "hosted_checkout".into()),
        ("client_attribution_metadata[payment_method_selection_flow]".into(), "merchant_specified".into()),
        ("client_attribution_metadata[checkout_config_id]".into(), config_id.to_string()),
    ];

    if !captcha_token.is_empty() {
        data.push(("radar_options[hcaptcha_token]".into(), captcha_token.to_string()));
    }

    let url = format!("{}/v1/payment_methods", STRIPE_API);
    emit_log(app, task_id, "info", "[4/6] 创建支付方式 (payment_method) ...");

    let resp = client.post(&url)
        .headers(stripe_headers())
        .form(&data)
        .send()
        .await
        .map_err(|e| format!("创建 payment_method 失败: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("创建 payment_method 失败 [{}]: {}", status, &body[..body.len().min(500)]));
    }

    let pm: serde_json::Value = resp.json().await
        .map_err(|e| format!("解析 payment_method 失败: {}", e))?;
    let pm_id = pm.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let brand = pm.pointer("/card/display_brand").and_then(|v| v.as_str()).unwrap_or("unknown");
    let last4 = pm.pointer("/card/last4").and_then(|v| v.as_str()).unwrap_or("????");
    emit_log(app, task_id, "info", &format!("  成功: {} ({} ****{})", pm_id, brand, last4));

    Ok(pm_id)
}

// ─── 确认支付 ─────────────────────────────────────────────────
async fn confirm_payment(
    client: &reqwest::Client,
    session_id: &str,
    pm_id: &str,
    captcha_token: &str,
    init_resp: &serde_json::Value,
    guid: &str,
    muid: &str,
    sid: &str,
    stripe_js_id: &str,
    elements_session_id: &str,
    captcha_cfg: &CaptchaConfig,
    px_tokens: &PxTokens,
    stripe_ver: &str,
    app: &AppHandle,
    task_id: &str,
) -> Result<serde_json::Value, String> {
    let init_checksum = init_resp.get("init_checksum").and_then(|v| v.as_str()).unwrap_or("");
    let config_id = init_resp.get("config_id").and_then(|v| v.as_str()).unwrap_or("");

    // 从 line_items 计算 expected_amount (与 Python 一致)
    let expected_amount = if let Some(items) = init_resp.get("line_items").and_then(|v| v.as_array()) {
        let total: i64 = items.iter().map(|item| item.get("amount").and_then(|a| a.as_i64()).unwrap_or(0)).sum();
        total.to_string()
    } else {
        "0".to_string()
    };

    // return_url (Python: init_resp.get("url") or init_resp.get("stripe_hosted_url"))
    let return_url = init_resp.get("url")
        .or(init_resp.get("stripe_hosted_url"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    emit_log(app, task_id, "debug", &format!(
        "  [confirm] init_checksum={}, config_id={}, pm={}, es_id={}, expected_amount={}",
        if init_checksum.is_empty() { "(空)" } else { &init_checksum[..init_checksum.len().min(20)] },
        if config_id.is_empty() { "(空)" } else { &config_id[..config_id.len().min(20)] },
        pm_id,
        if elements_session_id.is_empty() { "(空)" } else { &elements_session_id[..elements_session_id.len().min(20)] },
        expected_amount,
    ));

    let mut data: Vec<(String, String)> = vec![
        ("eid".into(), "NA".into()),
        ("payment_method".into(), pm_id.to_string()),
        ("expected_amount".into(), expected_amount),
        ("expected_payment_method_type".into(), "card".into()),
        ("guid".into(), guid.to_string()),
        ("muid".into(), muid.to_string()),
        ("sid".into(), sid.to_string()),
        ("key".into(), KNOWN_PK.into()),
        ("version".into(), stripe_ver.to_string()),
        ("init_checksum".into(), init_checksum.to_string()),
    ];

    // js_checksum 是 Stripe.js 完整性校验 (浏览器必发)
    if !px_tokens.js_checksum.is_empty() {
        data.push(("js_checksum".into(), px_tokens.js_checksum.clone()));
    }

    // 添加 PerimeterX 反机器人令牌 (浏览器在 confirm 中发送这些字段)
    if !px_tokens.px3.is_empty() {
        data.push(("px3".into(), px_tokens.px3.clone()));
    }
    if !px_tokens.pxvid.is_empty() {
        data.push(("pxvid".into(), px_tokens.pxvid.clone()));
    }
    if !px_tokens.pxcts.is_empty() {
        data.push(("pxcts".into(), px_tokens.pxcts.clone()));
    }

    // passive_captcha_token: 浏览器在 confirm 前自动解 hCaptcha 获取的被动令牌
    if !captcha_token.is_empty() {
        data.push(("passive_captcha_token".into(), captcha_token.to_string()));
    }
    // passive_captcha_ekey: 浏览器始终发送此字段 (通常为空)
    data.push(("passive_captcha_ekey".into(), "".into()));

    // rv_timestamp: Stripe.js 会话完整性时间戳
    if !px_tokens.rv_timestamp.is_empty() {
        data.push(("rv_timestamp".into(), px_tokens.rv_timestamp.clone()));
    }

    // client_attribution_metadata: 浏览器在 confirm 中也发送这些字段
    data.push(("client_attribution_metadata[client_session_id]".into(), stripe_js_id.to_string()));
    data.push(("client_attribution_metadata[checkout_session_id]".into(), session_id.to_string()));
    data.push(("client_attribution_metadata[merchant_integration_source]".into(), "checkout".into()));
    data.push(("client_attribution_metadata[merchant_integration_version]".into(), "hosted_checkout".into()));
    data.push(("client_attribution_metadata[payment_method_selection_flow]".into(), "merchant_specified".into()));
    data.push(("client_attribution_metadata[checkout_config_id]".into(), config_id.to_string()));

    let url = format!("{}/v1/payment_pages/{}/confirm", STRIPE_API, session_id);
    emit_log(app, task_id, "info", "[5/6] 确认支付 (confirm) ...");

    let resp = client.post(&url)
        .headers(stripe_headers())
        .form(&data)
        .send()
        .await
        .map_err(|e| format!("confirm 请求失败: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        // 解析 Stripe 错误详情
        let detail = if let Ok(err_json) = serde_json::from_str::<serde_json::Value>(&body) {
            let err_type = err_json.pointer("/error/type").and_then(|v| v.as_str()).unwrap_or("");
            let err_code = err_json.pointer("/error/code").and_then(|v| v.as_str()).unwrap_or("");
            let err_msg = err_json.pointer("/error/message").and_then(|v| v.as_str()).unwrap_or("");
            let decline = err_json.pointer("/error/decline_code").and_then(|v| v.as_str()).unwrap_or("");
            let param = err_json.pointer("/error/param").and_then(|v| v.as_str()).unwrap_or("");
            emit_log(app, task_id, "error", &format!(
                "  confirm 详细错误: type={}, code={}, decline={}, param={}, msg={}",
                err_type, err_code, decline, param, err_msg
            ));
            // 打印完整错误 JSON 方便调试
            emit_log(app, task_id, "debug", &format!(
                "  confirm 完整响应: {}", &body[..body.len().min(1500)]
            ));
            format!("type={}, code={}, param={}, msg={}", err_type, err_code, param, err_msg)
        } else {
            body[..body.len().min(500)].to_string()
        };
        return Err(format!("confirm 失败 [{}]: {}", status, detail));
    }

    let confirm_data: serde_json::Value = resp.json().await
        .map_err(|e| format!("解析 confirm 响应失败: {}", e))?;

    // 处理 3DS 验证
    let raw = serde_json::to_string(&confirm_data).unwrap_or_default();
    let has_next_action = confirm_data.get("next_action").is_some()
        || raw.contains("use_stripe_sdk");

    if has_next_action {
        emit_log(app, task_id, "info", "  触发 3DS 验证，正在处理 ...");
        handle_3ds(client, &confirm_data, captcha_token, captcha_cfg, guid, muid, sid, app, task_id).await?;
    }

    Ok(confirm_data)
}

// ─── 3DS 处理 ─────────────────────────────────────────────────
async fn handle_3ds(
    client: &reqwest::Client,
    confirm_data: &serde_json::Value,
    captcha_token: &str,
    captcha_cfg: &CaptchaConfig,
    _guid: &str,
    _muid: &str,
    _sid: &str,
    app: &AppHandle,
    task_id: &str,
) -> Result<(), String> {
    let raw = serde_json::to_string(confirm_data).unwrap_or_default();

    // 提取 source (setatt_xxx)
    let re_source = regex::Regex::new(r"(setatt_[A-Za-z0-9]+)").unwrap();
    let mut source = re_source.captures(&raw).map(|c| c[1].to_string());

    // 提取 seti_id
    let re_seti = regex::Regex::new(r"(seti_[A-Za-z0-9]+)").unwrap();
    let seti_id = re_seti.captures(&raw).map(|c| c[1].to_string());

    // 提取 client_secret
    let mut client_secret: Option<String> = None;
    if let Some(ref seti) = seti_id {
        let re_cs = regex::Regex::new(&format!(r"({}_secret_[A-Za-z0-9]+)", regex::escape(seti))).unwrap();
        client_secret = re_cs.captures(&raw).map(|c| c[1].to_string());
    }

    emit_log(app, task_id, "debug", &format!("  3DS: source={:?}, seti={:?}", source, seti_id));

    // 检查 challenge site_key
    let mut challenge_site_key: Option<String> = None;
    if let Some(na) = confirm_data.pointer("/setup_intent/next_action/use_stripe_sdk/stripe_js/site_key") {
        if let Some(sk) = na.as_str() {
            challenge_site_key = Some(sk.to_string());
        }
    }
    // Fallback: regex search for site_key
    if challenge_site_key.is_none() {
        let re_sk = regex::Regex::new(r#""site_key"\s*:\s*"([0-9a-f-]+)""#).unwrap();
        if let Some(cap) = re_sk.captures(&raw) {
            let sk = cap[1].to_string();
            if sk != HCAPTCHA_SITE_KEY_FALLBACK {
                challenge_site_key = Some(sk);
            }
        }
    }

    // 处理 challenge captcha
    if let (Some(ref sk), Some(ref seti), Some(ref cs)) = (&challenge_site_key, &seti_id, &client_secret) {
        let max_attempts = 2;
        for attempt in 1..=max_attempts {
            emit_log(app, task_id, "info", &format!("  解 challenge captcha (第 {}/{} 次) ...", attempt, max_attempts));
            let (challenge_token, _ekey) = solve_hcaptcha(captcha_cfg, sk, app, task_id).await?;

            let verify_url = format!("{}/v1/setup_intents/{}/verify_challenge", STRIPE_API, seti);
            let verify_data: Vec<(String, String)> = vec![
                ("client_secret".into(), cs.clone()),
                ("challenge_response_token".into(), challenge_token),
                ("captcha_vendor_name".into(), "hcaptcha".into()),
                ("key".into(), KNOWN_PK.into()),
                ("_stripe_version".into(), STRIPE_VERSION_BASE.into()),
            ];

            let resp = client.post(&verify_url)
                .headers(stripe_headers())
                .form(&verify_data)
                .send()
                .await
                .map_err(|e| format!("verify_challenge 失败: {}", e))?;

            if !resp.status().is_success() {
                let body = resp.text().await.unwrap_or_default();
                emit_log(app, task_id, "error", &format!("  verify_challenge 失败: {}", &body[..body.len().min(300)]));
                break;
            }

            let result: serde_json::Value = resp.json().await.unwrap_or_default();
            let verify_status = result.get("status").and_then(|v| v.as_str()).unwrap_or("unknown");
            emit_log(app, task_id, "info", &format!("  verify_challenge 状态: {}", verify_status));

            // requires_payment_method = 卡被拒绝，不是 captcha 问题，直接退出
            if verify_status == "requires_payment_method" {
                emit_log(app, task_id, "warn", "  卡被拒绝 (requires_payment_method)，退出 challenge 重试");
                break;
            }

            // 检查 captcha 错误
            let setup_error = result.get("last_setup_error");
            if let Some(err) = setup_error {
                let err_msg = err.get("message").and_then(|v| v.as_str()).unwrap_or("");
                if err_msg.to_lowercase().contains("captcha") {
                    if attempt < max_attempts {
                        emit_log(app, task_id, "warn", "  challenge captcha 被拒，重试...");
                        continue;
                    }
                    return Err(format!("challenge captcha 连续 {} 次被拒", max_attempts));
                }
            }

            // 提取新的 source
            let verify_raw = serde_json::to_string(&result).unwrap_or_default();
            if let Some(cap) = re_source.captures(&verify_raw) {
                source = Some(cap[1].to_string());
            }
            break;
        }
    } else if let (Some(ref seti), Some(ref cs)) = (&seti_id, &client_secret) {
        // Fallback: 用原始 captcha_token
        if source.is_none() && !captcha_token.is_empty() {
            let verify_url = format!("{}/v1/setup_intents/{}/verify_challenge", STRIPE_API, seti);
            let verify_data: Vec<(String, String)> = vec![
                ("client_secret".into(), cs.clone()),
                ("challenge_response_token".into(), captcha_token.to_string()),
                ("captcha_vendor_name".into(), "hcaptcha".into()),
                ("key".into(), KNOWN_PK.into()),
                ("_stripe_version".into(), STRIPE_VERSION_BASE.into()),
            ];

            let resp = client.post(&verify_url)
                .headers(stripe_headers())
                .form(&verify_data)
                .send()
                .await;

            if let Ok(r) = resp {
                if r.status().is_success() {
                    if let Ok(result) = r.json::<serde_json::Value>().await {
                        let verify_raw = serde_json::to_string(&result).unwrap_or_default();
                        if let Some(cap) = re_source.captures(&verify_raw) {
                            source = Some(cap[1].to_string());
                        }
                    }
                }
            }
        }
    }

    // 3DS2 authenticate
    if let Some(ref src) = source {
        // 等待指纹处理
        let wait: u64 = { rand::thread_rng().gen_range(5000..8000) };
        emit_log(app, task_id, "info", &format!("  [3ds] 等待指纹处理 ({:.1}s) ...", wait as f64 / 1000.0));
        tokio::time::sleep(std::time::Duration::from_millis(wait)).await;

        let auth_url = format!("{}/v1/3ds2/authenticate", STRIPE_API);
        emit_log(app, task_id, "info", &format!("  3DS2 authenticate (source: {}...)", &src[..src.len().min(30)]));

        let browser_json = json!({
            "fingerprintAttempted": true,
            "fingerprintData": null,
            "challengeWindowSize": null,
            "threeDSCompInd": "Y",
            "browserJavaEnabled": false,
            "browserJavascriptEnabled": true,
            "browserLanguage": "en-US",
            "browserColorDepth": "24",
            "browserScreenHeight": "1080",
            "browserScreenWidth": "1920",
            "browserTZ": "360",
            "browserUserAgent": USER_AGENT,
        });

        let auth_data: Vec<(String, String)> = vec![
            ("source".into(), src.clone()),
            ("browser".into(), serde_json::to_string(&browser_json).unwrap()),
            ("one_click_authn_device_support[hosted]".into(), "false".into()),
            ("one_click_authn_device_support[same_origin_frame]".into(), "false".into()),
            ("one_click_authn_device_support[spc_eligible]".into(), "true".into()),
            ("one_click_authn_device_support[webauthn_eligible]".into(), "true".into()),
            ("one_click_authn_device_support[publickey_credentials_get_allowed]".into(), "true".into()),
            ("key".into(), KNOWN_PK.into()),
            ("_stripe_version".into(), STRIPE_VERSION_BASE.into()),
        ];

        let resp = client.post(&auth_url)
            .headers(stripe_headers())
            .form(&auth_data)
            .send()
            .await;

        match resp {
            Ok(r) if r.status().is_success() => {
                let result: serde_json::Value = r.json().await.unwrap_or_default();
                let state = result.get("state").and_then(|v| v.as_str()).unwrap_or("unknown");

                // 关键：检查 transStatus 字段
                let trans_status = result.pointer("/ares/transStatus")
                    .or_else(|| result.get("transStatus"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                emit_log(app, task_id, "info", &format!("  3DS2 结果: state={}, transStatus={}", state, if trans_status.is_empty() { "N/A" } else { trans_status }));

                match trans_status {
                    "Y" => {
                        emit_log(app, task_id, "info", "  3DS 验证通过 (transStatus=Y)");
                    }
                    "C" => {
                        return Err("3DS 需要银行短信验证 (transStatus=C)，此卡无法自动绑定".to_string());
                    }
                    "R" => {
                        return Err("3DS 验证被拒绝 (transStatus=R)".to_string());
                    }
                    "N" => {
                        emit_log(app, task_id, "warn", "  3DS 验证未通过 (transStatus=N)，继续尝试...");
                    }
                    "" => {
                        // transStatus 为空，检查 state 是否已成功
                        if state == "succeeded" || state == "challenge_required" {
                            emit_log(app, task_id, "info", &format!("  3DS2 state={}, 继续流程", state));
                        } else {
                            emit_log(app, task_id, "warn", &format!("  3DS2 无 transStatus, state={}", state));
                        }
                    }
                    other => {
                        emit_log(app, task_id, "warn", &format!("  3DS transStatus={}, 继续尝试...", other));
                    }
                }
            }
            Ok(r) => {
                let body = r.text().await.unwrap_or_default();
                emit_log(app, task_id, "warn", &format!("  3DS2 authenticate 失败: {}", &body[..body.len().min(200)]));
            }
            Err(e) => {
                emit_log(app, task_id, "warn", &format!("  3DS2 authenticate 异常: {}", e));
            }
        }
    } else {
        emit_log(app, task_id, "warn", "  没有 setatt_ source, 跳过 3DS2 authenticate");
    }

    // 查询 setup_intent 状态
    if let (Some(ref seti), Some(ref cs)) = (&seti_id, &client_secret) {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let poll_url = format!("{}/v1/setup_intents/{}", STRIPE_API, seti);
        let params = [
            ("client_secret", cs.as_str()),
            ("is_stripe_sdk", "false"),
            ("key", KNOWN_PK),
            ("_stripe_version", STRIPE_VERSION_BASE),
        ];

        let resp = client.get(&poll_url)
            .headers(stripe_headers())
            .query(&params)
            .send()
            .await;

        if let Ok(r) = resp {
            if r.status().is_success() {
                if let Ok(result) = r.json::<serde_json::Value>().await {
                    let status = result.get("status").and_then(|v| v.as_str()).unwrap_or("unknown");
                    emit_log(app, task_id, "info", &format!("  setup_intent 状态: {}", status));

                    if status == "requires_action" {
                        emit_log(app, task_id, "warn", "  setup_intent 仍需操作，此卡可能需要额外验证");
                    }
                }
            }
        }
    } else if seti_id.is_none() {
        // 开发者建议: seti 为 None 时直接放弃
        emit_log(app, task_id, "warn", "  setup_intent 为空，3DS 流程异常");
        return Err("3DS: 未获取到 setup_intent，无法完成验证".to_string());
    }

    Ok(())
}

// ─── 轮询结果 ─────────────────────────────────────────────────
async fn poll_result(
    client: &reqwest::Client,
    session_id: &str,
    app: &AppHandle,
    task_id: &str,
) -> Result<serde_json::Value, String> {
    let url = format!("{}/v1/payment_pages/{}/poll", STRIPE_API, session_id);
    let params = [
        ("key", KNOWN_PK),
        ("_stripe_version", STRIPE_VERSION_BASE),
    ];

    emit_log(app, task_id, "info", "[6/6] 轮询支付结果 ...");

    for attempt in 0..30 {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let resp = client.get(&url)
            .headers(stripe_headers())
            .query(&params)
            .send()
            .await;

        let r = match resp {
            Ok(r) => r,
            Err(_) => continue,
        };

        if !r.status().is_success() {
            emit_log(app, task_id, "warn", &format!("  poll 返回 {}, 重试...", r.status()));
            continue;
        }

        let data: serde_json::Value = match r.json().await {
            Ok(d) => d,
            Err(_) => continue,
        };

        let state = data.get("state").and_then(|v| v.as_str()).unwrap_or("unknown");
        let payment_status = data.get("payment_object_status").and_then(|v| v.as_str()).unwrap_or("unknown");

        if state == "succeeded" {
            emit_log(app, task_id, "info", &format!("  成功! state={}, payment_status={}", state, payment_status));
            return Ok(data);
        }

        if state == "failed" || state == "expired" || state == "canceled" {
            emit_log(app, task_id, "error", &format!("  失败: state={}", state));
            return Ok(data);
        }

        // requires_payment_method = 卡被拒绝，不用再轮询
        if payment_status == "requires_payment_method" {
            emit_log(app, task_id, "warn", &format!("  卡被拒绝 (payment_status=requires_payment_method)，退出轮询"));
            return Ok(data);
        }

        if attempt % 3 == 2 {
            emit_log(app, task_id, "info", &format!("  state={}, payment_status={} ({}/30)", state, payment_status, attempt + 1));
        }
    }

    Err("轮询超时 (60s)".to_string())
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 单账号完整绑卡流程
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
async fn bind_single_account(
    account_id: &str,
    email: &str,
    stripe_url: &str,
    card: &CardInfo,
    captcha_cfg: &CaptchaConfig,
    proxy: &Option<ProxyConfig>,
    custom_address: &Option<AddressInfo>,
    custom_name: &Option<String>,
    presolve_captcha: bool,
    app: &AppHandle,
    task_id: &str,
) -> Result<serde_json::Value, String> {
    let client = build_stripe_client(proxy)?;
    let addr = custom_address.clone().unwrap_or_else(generate_random_address);
    let name = custom_name.clone().unwrap_or_else(generate_random_name);
    let random_email = generate_random_email();
    let stripe_js_id = Uuid::new_v4().to_string();

    emit_log(app, task_id, "info", &format!("═══ 开始绑卡: {} ═══", email));
    emit_log(app, task_id, "info", &format!("  卡号: ****{}", &card.number[card.number.len().saturating_sub(4)..]));
    emit_log(app, task_id, "info", &format!("  姓名: {}, 地址: {}, {}, {} {}", name, addr.line1, addr.city, addr.state, addr.postal_code));

    // Step 1: 解析 session_id
    emit_progress(app, task_id, account_id, 1, "解析Session", "running");
    emit_log(app, task_id, "info", "[1/6] 解析 checkout session ID ...");
    let session_id = parse_session_id(stripe_url)?;
    emit_log(app, task_id, "info", &format!("  session_id: {}", session_id));

    // Step 2: 注册指纹 & init checkout
    emit_progress(app, task_id, account_id, 2, "初始化", "running");
    let (guid, muid, sid) = register_fingerprint(&client, app, task_id).await?;

    emit_log(app, task_id, "info", "[2/6] 初始化 checkout ...");
    let (init_resp, stripe_ver) = init_checkout(&client, &session_id, app, task_id).await?;

    let display_name = init_resp.pointer("/account_settings/display_name")
        .and_then(|v| v.as_str()).unwrap_or("?");
    emit_log(app, task_id, "info", &format!("  商户: {}", display_name));

    // Step 2c: 获取 elements session (与 Python 一致)
    let (elements_session_id, _es_config_id) = fetch_elements_session(
        &client, &session_id, &stripe_js_id, &stripe_ver, app, task_id,
    ).await?;

    // Step 3: 提交地址
    emit_progress(app, task_id, account_id, 3, "提交地址", "running");
    emit_log(app, task_id, "info", "[3/6] 提交账单地址 ...");
    update_address(&client, &session_id, &addr, &stripe_js_id, &elements_session_id, &stripe_ver, app, task_id).await?;

    // Step 4: 解 captcha + 创建 payment_method
    emit_progress(app, task_id, account_id, 4, "创建支付方式", "running");
    let (hcaptcha_site_key, _rqdata) = extract_hcaptcha_config(&init_resp);
    let config_id = init_resp.get("config_id").and_then(|v| v.as_str()).unwrap_or("");

    let (captcha_token, pm_id) = if presolve_captcha {
        // 预解模式: 先解 hCaptcha，带 token 提交降低风控评级
        emit_log(app, task_id, "info", "[3.5/6] 解 hCaptcha ...");
        match solve_hcaptcha(captcha_cfg, &hcaptcha_site_key, app, task_id).await {
            Ok((token, _ekey)) => {
                let pm = create_payment_method(
                    &client, card, &addr, &name, email, &token, &session_id,
                    config_id, &stripe_js_id, &guid, &muid, &sid, app, task_id,
                ).await?;
                (token, pm)
            }
            Err(captcha_err) => {
                emit_log(app, task_id, "warn", &format!("  hCaptcha 失败: {}, 尝试不带 captcha 提交 ...", &captcha_err[..captcha_err.len().min(80)]));
                let pm = create_payment_method(
                    &client, card, &addr, &name, email, "", &session_id,
                    config_id, &stripe_js_id, &guid, &muid, &sid, app, task_id,
                ).await?;
                ("".to_string(), pm)
            }
        }
    } else {
        // 非预解模式: 先不带 captcha 提交
        emit_log(app, task_id, "info", "[3.5/6] 尝试不带 hCaptcha 直接提交 ...");
        match create_payment_method(
            &client, card, &addr, &name, email, "", &session_id,
            config_id, &stripe_js_id, &guid, &muid, &sid, app, task_id,
        ).await {
            Ok(pm) => ("".to_string(), pm),
            Err(e) => {
                let err_lower = e.to_lowercase();
                if err_lower.contains("captcha") || err_lower.contains("hcaptcha")
                    || err_lower.contains("blocked") || err_lower.contains("radar") {
                    emit_log(app, task_id, "info", "  需要 hCaptcha，开始解题 ...");
                    let (token, _ekey) = solve_hcaptcha(captcha_cfg, &hcaptcha_site_key, app, task_id).await?;
                    let pm = create_payment_method(
                        &client, card, &addr, &name, email, &token, &session_id,
                        config_id, &stripe_js_id, &guid, &muid, &sid, app, task_id,
                    ).await?;
                    (token, pm)
                } else {
                    return Err(e);
                }
            }
        }
    };

    // Step 4.5: 获取 PX 反机器人令牌 (在 confirm 之前)
    emit_log(app, task_id, "info", "[4.5/6] 获取反机器人令牌 ...");
    let px_tokens = match fetch_px_tokens(stripe_url, app, task_id).await {
        Ok(t) => t,
        Err(e) => {
            emit_log(app, task_id, "warn", &format!("  PX 令牌获取失败: {}, 将不带令牌继续", &e[..e.len().min(100)]));
            PxTokens::default()
        }
    };

    // 获取当前 stripe.js 版本 (优先用 PX 浏览器提取的，否则动态获取)
    let dynamic_stripe_ver = if !px_tokens.stripe_version_hash.is_empty() {
        px_tokens.stripe_version_hash.clone()
    } else {
        fetch_stripe_js_version().await
    };
    emit_log(app, task_id, "debug", &format!("  stripe.js version: {}", dynamic_stripe_ver));

    // 确保 confirm 前有 passive_captcha_token (浏览器始终发送此字段)
    let passive_captcha_token = if captcha_token.is_empty() {
        emit_log(app, task_id, "info", "[4.6/6] 解被动 hCaptcha (用于 confirm) ...");
        match solve_hcaptcha(captcha_cfg, HCAPTCHA_SITE_KEY_FALLBACK, app, task_id).await {
            Ok((token, _ekey)) => {
                emit_log(app, task_id, "info", &format!("  被动 captcha 已解决 ({}字符)", token.len()));
                token
            }
            Err(e) => {
                emit_log(app, task_id, "warn", &format!("  被动 captcha 失败: {}, 继续...", &e[..e.len().min(80)]));
                String::new()
            }
        }
    } else {
        captcha_token.clone()
    };

    // Step 5: confirm
    emit_progress(app, task_id, account_id, 5, "确认支付", "running");
    let confirm_result = confirm_payment(
        &client, &session_id, &pm_id, &passive_captcha_token, &init_resp,
        &guid, &muid, &sid, &stripe_js_id, &elements_session_id,
        captcha_cfg, &px_tokens, &dynamic_stripe_ver, app, task_id,
    ).await;

    match confirm_result {
        Ok(_) => {},
        Err(ref e) if !presolve_captcha && captcha_token.is_empty() && {
            let el = e.to_lowercase();
            el.contains("captcha") || el.contains("blocked") || el.contains("radar")
        } => {
            emit_log(app, task_id, "info", "  confirm 需要 hCaptcha，开始解题后重试 ...");
            let (solved_token, _ekey) = solve_hcaptcha(captcha_cfg, &hcaptcha_site_key, app, task_id).await?;
            confirm_payment(
                &client, &session_id, &pm_id, &solved_token, &init_resp,
                &guid, &muid, &sid, &stripe_js_id, &elements_session_id,
                captcha_cfg, &px_tokens, &dynamic_stripe_ver, app, task_id,
            ).await?;
        },
        Err(e) => return Err(e),
    }

    // Step 6: poll
    emit_progress(app, task_id, account_id, 6, "轮询结果", "running");
    let result = poll_result(&client, &session_id, app, task_id).await?;

    let state = result.get("state").and_then(|v| v.as_str()).unwrap_or("unknown");
    if state == "succeeded" {
        emit_progress(app, task_id, account_id, 6, "成功", "success");
    } else {
        emit_progress(app, task_id, account_id, 6, &format!("失败: {}", state), "failed");
    }

    Ok(result)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Tauri 命令
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 生成随机美国地址（前端预览用）
#[tauri::command]
pub async fn stripe_bind_generate_address() -> Result<serde_json::Value, String> {
    let addr = generate_random_address();
    let name = generate_random_name();
    Ok(json!({
        "name": name,
        "address": {
            "country": addr.country,
            "line1": addr.line1,
            "city": addr.city,
            "state": addr.state,
            "postal_code": addr.postal_code,
        }
    }))
}

/// 启动批量绑卡任务
#[tauri::command]
pub async fn stripe_bind_start(
    app: AppHandle,
    request: StripeBindRequest,
    store: State<'_, Arc<DataStore>>,
) -> Result<serde_json::Value, String> {
    let batch_id = Uuid::new_v4().to_string();
    let concurrency = request.concurrency.unwrap_or(1).max(1).min(5);
    let total = request.account_ids.len();

    let debug_cards = request.debug_cards.clone().unwrap_or_default();
    let is_debug_mode = !debug_cards.is_empty();
    let max_debug_failures = request.max_debug_failures.unwrap_or(25);

    if !is_debug_mode && request.cards.is_empty() {
        return Err("请至少添加一张卡".to_string());
    }
    if is_debug_mode && debug_cards.is_empty() {
        return Err("调试模式需要至少一张调试卡".to_string());
    }
    let card_count = if is_debug_mode { debug_cards.len() } else { request.cards.len() };

    emit_log(&app, &batch_id, "info", &format!(
        "═══════════════════════════════════════════════════════════════"));
    if is_debug_mode {
        emit_log(&app, &batch_id, "info", &format!(
            "  🔧 调试绑卡模式 — {} 个账号, {} 张调试卡, 最大失败 {} 次, 并发 {}", total, card_count, max_debug_failures, concurrency));
    } else {
        emit_log(&app, &batch_id, "info", &format!(
            "  批量协议绑卡任务启动 — 共 {} 个账号, {} 张卡, 并发 {}", total, card_count, concurrency));
    }
    emit_log(&app, &batch_id, "info", &format!(
        "═══════════════════════════════════════════════════════════════"));

    // 先为每个账号获取 trial payment link (需要 Windsurf token)
    let mut tasks: Vec<(String, String, String)> = Vec::new(); // (account_id, email, stripe_url)

    let teams_tier = request.teams_tier.unwrap_or(2); // 默认 Pro
    let payment_period = request.payment_period.unwrap_or(1); // 默认月付

    for account_id_str in &request.account_ids {
        let uuid = Uuid::parse_str(account_id_str).map_err(|e| e.to_string())?;
        let mut account = store.get_account(uuid).await.map_err(|e| e.to_string())?;

        // 确保 token 有效
        super::api_commands::ensure_valid_token(&store, &mut account, uuid).await?;
        let token = account.token.clone().ok_or("No token available")?;

        emit_log(&app, &batch_id, "info", &format!("  获取试用链接: {} ...", account.email));

        // 从 turnstile_tokens map 获取该账号的 token
        let turnstile_token = request.turnstile_tokens.as_ref()
            .and_then(|m| m.get(account_id_str))
            .cloned();

        let windsurf_service = WindsurfService::new();
        let auth1 = account.refresh_token.as_deref().filter(|t| t.starts_with("auth1_"));
        let result = windsurf_service.subscribe_to_plan(
            &token, auth1, teams_tier, payment_period, None, None, turnstile_token.as_deref(),
        ).await.map_err(|e: AppError| format!("获取试用链接失败 ({}): {}", account.email, e))?;

        // 检查API是否成功
        let api_success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
        if !api_success {
            let api_err = result.get("error").and_then(|v| v.as_str()).unwrap_or("未知错误");
            let status_code = result.get("status_code").and_then(|v| v.as_u64()).unwrap_or(0);
            emit_log(&app, &batch_id, "error", &format!("  ❌ {} 获取链接失败: {} (status={})", account.email, api_err, status_code));
            return Err(format!("获取试用链接失败 ({}): {}", account.email, api_err));
        }

        let stripe_url = result.get("stripe_url")
            .and_then(|v| v.as_str())
            .ok_or(format!("未获取到 stripe_url ({})", account.email))?
            .to_string();

        emit_log(&app, &batch_id, "info", &format!("  ✅ {} → {}", account.email, &stripe_url[..stripe_url.len().min(80)]));

        // 初始化任务状态
        let task_state = BindTaskState {
            task_id: batch_id.clone(),
            account_id: account_id_str.clone(),
            email: account.email.clone(),
            status: "pending".to_string(),
            step: 0,
            step_name: "等待中".to_string(),
            error: None,
            stripe_url: Some(stripe_url.clone()),
        };
        BIND_TASKS.lock().await.insert(
            format!("{}_{}", batch_id, account_id_str),
            task_state,
        );

        tasks.push((account_id_str.clone(), account.email.clone(), stripe_url));
    }

    // 在后台异步执行绑卡
    let cards = request.cards.clone();
    let captcha_cfg = request.captcha.clone();
    let proxy = request.proxy.clone();
    let custom_address = request.custom_address.clone();
    let custom_name = request.custom_name.clone();
    let batch_id_clone = batch_id.clone();
    let app_clone = app.clone();
    let store_inner = store.inner().clone();
    let presolve_captcha = request.presolve_captcha.unwrap_or(false);
    let debug_cards_arc = Arc::new(debug_cards);
    let failure_counter = Arc::new(AtomicU32::new(0));

    tokio::spawn(async move {
        let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));
        let mut handles = Vec::new();

        for (idx, (account_id, email, stripe_url)) in tasks.into_iter().enumerate() {
            let sem = semaphore.clone();
            let card = if !is_debug_mode { Some(cards[idx % cards.len()].clone()) } else { None };
            let debug_cards = debug_cards_arc.clone();
            let failure_counter = failure_counter.clone();
            let captcha_cfg = captcha_cfg.clone();
            let proxy = proxy.clone();
            let custom_address = custom_address.clone();
            let custom_name = custom_name.clone();
            let batch_id = batch_id_clone.clone();
            let app = app_clone.clone();
            let store = store_inner.clone();

            let handle = tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                let task_key = format!("{}_{}", batch_id, account_id);

                // 更新状态: running
                {
                    let mut tasks = BIND_TASKS.lock().await;
                    if let Some(t) = tasks.get_mut(&task_key) {
                        t.status = "running".to_string();
                    }
                }

                let (status, error) = if is_debug_mode {
                    // ─── 调试模式: 逐卡尝试 ───
                    let mut final_status = "failed".to_string();
                    let mut final_error: Option<String> = Some("所有调试卡均失败".into());

                    for (card_idx, dcard) in debug_cards.iter().enumerate() {
                        let current_failures = failure_counter.load(Ordering::Relaxed);
                        if current_failures >= max_debug_failures {
                            emit_log(&app, &batch_id, "warn", &format!(
                                "  ⚠ 已达最大失败次数 ({}/{}), 跳过剩余卡片", current_failures, max_debug_failures));
                            final_error = Some(format!("已达最大失败次数 {}", max_debug_failures));
                            break;
                        }

                        let last4 = &dcard.number[dcard.number.len().saturating_sub(4)..];
                        emit_log(&app, &batch_id, "info", &format!(
                            "  🔧 [{}] 尝试调试卡 {}/{}: ****{}", email, card_idx + 1, debug_cards.len(), last4));

                        let result = bind_single_account(
                            &account_id, &email, &stripe_url,
                            dcard, &captcha_cfg, &proxy,
                            &custom_address, &custom_name, presolve_captcha, &app, &batch_id,
                        ).await;

                        match &result {
                            Ok(data) => {
                                let state = data.get("state").and_then(|v| v.as_str()).unwrap_or("unknown");
                                if state == "succeeded" {
                                    emit_log(&app, &batch_id, "info", &format!(
                                        "  ✅ [{}] 调试卡 ****{} 绑卡成功!", email, last4));
                                    // 通知前端: 这张调试卡成功了
                                    let _ = app.emit("stripe-bind-card-success", json!({
                                        "task_id": batch_id,
                                        "account_id": account_id,
                                        "card_index": card_idx,
                                        "card_number": dcard.number,
                                        "card_cvc": dcard.cvc,
                                        "card_exp_year": dcard.exp_year,
                                        "card_exp_month": dcard.exp_month,
                                    }));
                                    final_status = "success".to_string();
                                    final_error = None;
                                    break;
                                } else {
                                    let fail_count = failure_counter.fetch_add(1, Ordering::Relaxed) + 1;
                                    emit_log(&app, &batch_id, "warn", &format!(
                                        "  ❌ [{}] 调试卡 ****{} 失败: state={} (累计失败 {}/{})",
                                        email, last4, state, fail_count, max_debug_failures));
                                    let _ = app.emit("stripe-bind-card-failed", json!({
                                        "task_id": batch_id,
                                        "account_id": account_id,
                                        "card_index": card_idx,
                                        "card_number": dcard.number,
                                        "card_cvc": dcard.cvc,
                                        "error": format!("state={}", state),
                                    }));
                                }
                            }
                            Err(e) => {
                                let fail_count = failure_counter.fetch_add(1, Ordering::Relaxed) + 1;
                                emit_log(&app, &batch_id, "warn", &format!(
                                    "  ❌ [{}] 调试卡 ****{} 出错: {} (累计失败 {}/{})",
                                    email, last4, &e[..e.len().min(100)], fail_count, max_debug_failures));
                                let _ = app.emit("stripe-bind-card-failed", json!({
                                    "task_id": batch_id,
                                    "account_id": account_id,
                                    "card_index": card_idx,
                                    "card_number": dcard.number,
                                    "card_cvc": dcard.cvc,
                                    "error": &e[..e.len().min(100)],
                                }));
                                // 如果是 session 级别的错误(expired/canceled), 停止该账号
                                let el = e.to_lowercase();
                                if el.contains("expired") || el.contains("canceled") || el.contains("cancelled") {
                                    emit_log(&app, &batch_id, "error", &format!(
                                        "  ⛔ [{}] checkout session 已失效, 停止该账号", email));
                                    final_error = Some(format!("session 已失效: {}", &e[..e.len().min(80)]));
                                    break;
                                }
                            }
                        }
                    }
                    (final_status, final_error)
                } else {
                    // ─── 普通模式: 单卡绑定 ───
                    let card = card.as_ref().unwrap();
                    let result = bind_single_account(
                        &account_id, &email, &stripe_url,
                        card, &captcha_cfg, &proxy,
                        &custom_address, &custom_name, presolve_captcha, &app, &batch_id,
                    ).await;

                    match &result {
                        Ok(data) => {
                            let state = data.get("state").and_then(|v| v.as_str()).unwrap_or("unknown");
                            if state == "succeeded" {
                                ("success".to_string(), None)
                            } else {
                                ("failed".to_string(), Some(format!("state={}", state)))
                            }
                        }
                        Err(e) => ("failed".to_string(), Some(e.clone())),
                    }
                };

                // 更新状态
                {
                    let mut tasks = BIND_TASKS.lock().await;
                    if let Some(t) = tasks.get_mut(&task_key) {
                        t.status = status.clone();
                        t.error = error.clone();
                    }
                }

                // 记录日志到数据库
                let log = OperationLog::new(
                    OperationType::GetAccountInfo,
                    if status == "success" { OperationStatus::Success } else { OperationStatus::Failed },
                    format!("协议绑卡{}: {}{}", 
                        if status == "success" { "成功" } else { "失败" },
                        email,
                        error.as_ref().map(|e| format!(" ({})", e)).unwrap_or_default()
                    ),
                );
                let _ = store.add_log(log).await;

                // 通知前端任务完成
                let _ = app.emit("stripe-bind-task-done", json!({
                    "task_id": batch_id,
                    "account_id": account_id,
                    "email": email,
                    "status": status,
                    "error": error,
                }));
            });

            handles.push(handle);
        }

        // 等待所有任务完成
        for handle in handles {
            let _ = handle.await;
        }

        if is_debug_mode {
            let total_failures = failure_counter.load(Ordering::Relaxed);
            emit_log(&app_clone, &batch_id_clone, "info", &format!(
                "═══ 调试绑卡完成 (累计失败: {}/{}) ═══", total_failures, max_debug_failures));
        } else {
            emit_log(&app_clone, &batch_id_clone, "info", "═══ 所有绑卡任务已完成 ═══");
        }

        // 通知前端批次完成
        let _ = app_clone.emit("stripe-bind-batch-done", json!({
            "task_id": batch_id_clone,
        }));
    });

    Ok(json!({
        "success": true,
        "batch_id": batch_id,
        "total": total,
        "concurrency": concurrency,
    }))
}

/// 获取绑卡任务状态
#[tauri::command]
pub async fn stripe_bind_get_status(batch_id: String) -> Result<serde_json::Value, String> {
    let tasks = BIND_TASKS.lock().await;
    let results: Vec<&BindTaskState> = tasks.values()
        .filter(|t| t.task_id == batch_id)
        .collect();

    let total = results.len();
    let success = results.iter().filter(|t| t.status == "success").count();
    let failed = results.iter().filter(|t| t.status == "failed").count();
    let running = results.iter().filter(|t| t.status == "running").count();
    let pending = results.iter().filter(|t| t.status == "pending").count();

    Ok(json!({
        "batch_id": batch_id,
        "total": total,
        "success": success,
        "failed": failed,
        "running": running,
        "pending": pending,
        "tasks": results,
    }))
}

/// 取消绑卡任务
#[tauri::command]
pub async fn stripe_bind_cancel(batch_id: String) -> Result<serde_json::Value, String> {
    let mut tasks = BIND_TASKS.lock().await;
    let mut cancelled = 0;
    for task in tasks.values_mut() {
        if task.task_id == batch_id && task.status == "pending" {
            task.status = "cancelled".to_string();
            cancelled += 1;
        }
    }
    Ok(json!({
        "success": true,
        "cancelled": cancelled,
    }))
}
