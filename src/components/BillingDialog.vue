<template>
  <el-dialog
    v-model="visible"
    title="账单与订阅"
    width="800px"
    class="billing-dialog"
    :close-on-click-modal="false"
    @close="handleClose"
  >
    <div v-if="loading" class="loading-container">
      <el-icon class="is-loading" size="32"><Loading /></el-icon>
      <p>正在获取账单信息...</p>
    </div>
    
    <div v-else-if="billingData" class="billing-content">
      <!-- 速览面板：卡号 + 订阅类型 + 到期（精简核心 3 件套，无需打开浏览器） -->
      <div class="quick-summary">
        <div class="qs-item qs-card">
          <div class="qs-icon"><el-icon><CreditCard /></el-icon></div>
          <div class="qs-body">
            <div class="qs-label">卡号</div>
            <div class="qs-value mono" v-if="summaryCard.last4">
              <span class="qs-brand">{{ formatPaymentType(summaryCard.brand || '') }}</span>
              <span class="qs-mask">•••• {{ summaryCard.last4 }}</span>
              <span class="qs-exp" v-if="summaryCard.exp_month">
                ({{ String(summaryCard.exp_month).padStart(2, '0') }}/{{ summaryCard.exp_year }})
              </span>
            </div>
            <div class="qs-value qs-empty" v-else>未绑定</div>
          </div>
        </div>
        <div class="qs-item qs-plan">
          <div class="qs-icon"><el-icon><Trophy /></el-icon></div>
          <div class="qs-body">
            <div class="qs-label">订阅类型</div>
            <div class="qs-value">
              <span class="qs-plan-name">{{ summaryPlanName }}</span>
              <el-tag v-if="billingData.on_trial" type="warning" effect="light" size="small" round>试用</el-tag>
              <el-tag v-else-if="billingData.subscription_active" type="success" effect="light" size="small" round>活跃</el-tag>
              <el-tag v-else-if="billingData.subscription_active === false" type="info" effect="light" size="small" round>未激活</el-tag>
              <el-tag v-if="billingData.cancel_at_period_end" type="danger" effect="light" size="small" round>期末取消</el-tag>
            </div>
          </div>
        </div>
        <div class="qs-item qs-expiry">
          <div class="qs-icon"><el-icon><Clock /></el-icon></div>
          <div class="qs-body">
            <div class="qs-label">{{ summaryExpiryLabel }}</div>
            <div class="qs-value mono">{{ summaryExpiryValue }}</div>
          </div>
        </div>
      </div>

      <!-- 顶部订阅卡片 -->
      <div class="subscription-card" :class="`plan-${summaryPlanCode}`">
        <div class="card-bg-icon">
          <el-icon><Trophy /></el-icon>
        </div>
        <div class="sub-header">
          <div class="plan-info">
            <div class="plan-name">
              <el-icon><Trophy /></el-icon>
              {{ summaryPlanName }}
            </div>
            <div class="plan-status">
              <el-tag v-if="billingData.on_trial" type="warning" effect="dark" round size="small">试用期</el-tag>
              <el-tag v-if="billingData.subscription_active" type="success" effect="dark" round size="small">活跃</el-tag>
              <el-tag v-else-if="billingData.subscription_active === false" type="danger" effect="dark" round size="small">未激活</el-tag>
              <el-tag v-if="billingData.cancel_at_period_end" type="danger" effect="dark" round size="small">将于本期结束后取消</el-tag>
            </div>
          </div>
          <div class="sub-price" v-if="billingData.plan_unit_amount">
            <span class="currency">$</span>
            <span class="amount">{{ billingData.plan_unit_amount.toFixed(2) }}</span>
            <span class="unit" v-if="billingData.sub_interval"> / {{ billingData.sub_interval === 'yearly' ? '年' : '月' }}</span>
          </div>
        </div>
        
        <div class="sub-dates" v-if="billingData.next_billing_date || billingData.subscription_renewal_time">
          <div class="date-item" v-if="billingData.subscription_renewal_time">
            <span class="label">续期时间</span>
            <span class="value">{{ billingData.subscription_renewal_time }}</span>
          </div>
          <div class="date-item" v-if="billingData.next_billing_date">
            <span class="label">下次扣费</span>
            <span class="value">{{ billingData.next_billing_date }}</span>
          </div>
        </div>
      </div>

      <div class="info-grid">
        <!-- 席位使用情况 -->
        <div class="info-card seats-card" v-if="billingData.num_seats || billingData.num_users">
          <div class="card-title">
            <el-icon><User /></el-icon>
            <span>席位使用</span>
          </div>
          <div class="card-content">
            <div class="usage-circle-container">
               <el-progress 
                type="dashboard" 
                :percentage="getSeatUsagePercentage()" 
                :color="getSeatUsageColor()"
                :width="120"
                :stroke-width="10"
              >
                <template #default="{ percentage }">
                  <div class="percentage-value">{{ percentage }}%</div>
                  <div class="percentage-label">已使用</div>
                </template>
              </el-progress>
            </div>
            <div class="usage-details">
              <div class="detail-row">
                <span class="label">总席位</span>
                <span class="value">{{ billingData.num_users || 0 }} / {{ billingData.num_seats || 0 }}</span>
              </div>
              <div class="detail-row" v-if="billingData.num_cascade_users !== undefined">
                <span class="label">Cascade</span>
                <span class="value">{{ billingData.num_cascade_users || 0 }} / {{ billingData.num_cascade_seats || '-' }}</span>
              </div>
              <div class="detail-row" v-if="billingData.num_core_users !== undefined">
                <span class="label">Core</span>
                <span class="value">{{ billingData.num_core_users || 0 }} / {{ billingData.num_core_seats || '-' }}</span>
              </div>
            </div>
          </div>
        </div>
        
        <!-- 配额使用情况 -->
        <div class="info-card quota-card" v-if="billingData.total_quota">
          <div class="card-title">
            <el-icon><DataAnalysis /></el-icon>
            <span>配额使用</span>
          </div>
          <div class="card-content">
            <div class="quota-main">
              <div class="quota-text">
                <span class="current">{{ formatQuota(billingData.used_quota) }}</span>
                <span class="divider">/</span>
                <span class="total">{{ formatQuota(billingData.total_quota) }}</span>
              </div>
              <el-progress 
                :percentage="quotaPercentage"
                :stroke-width="12"
                :color="quotaColor"
                :show-text="false"
                class="quota-bar"
              />
            </div>
            
            <div class="quota-tags">
              <el-tag size="small" type="info" effect="plain" v-if="billingData.base_quota">
                基础: {{ formatQuota(billingData.base_quota) }}
              </el-tag>
              <el-tag size="small" type="success" effect="plain" v-if="billingData.extra_credits">
                额外: +{{ formatQuota(billingData.extra_credits) }}
              </el-tag>
            </div>

            <div class="cache-info" v-if="billingData.cache_limit">
              <div class="cache-header">
                <span>缓存使用 ({{ getCacheUsagePercentage() }}%)</span>
                <span>{{ formatQuota(billingData.cache_limit) }}</span>
              </div>
              <el-progress 
                :percentage="getCacheUsagePercentage()"
                :stroke-width="6"
                :color="getCacheUsageType() === 'danger' ? '#f56c6c' : (getCacheUsageType() === 'warning' ? '#e6a23c' : '#67c23a')"
                :show-text="false"
              />
            </div>
          </div>
        </div>
        
        <!-- 支付信息 -->
        <div class="info-card payment-card" v-if="billingData.payment_method || billingData.plan_unit_amount || billingData.is_new_account">
          <div class="card-title">
            <el-icon><CreditCard /></el-icon>
            <span>支付方式</span>
          </div>
          <div class="card-content">
            <div class="payment-method" v-if="billingData.payment_method">
              <div class="method-icon">
                <el-icon><CreditCard /></el-icon>
              </div>
              <div class="method-info">
                <div class="method-type">{{ formatPaymentType(billingData.payment_method.type) }}</div>
                <div class="method-number" v-if="billingData.payment_method?.last4">**** {{ billingData.payment_method.last4 }}</div>
                <div class="method-exp" v-if="billingData.payment_method?.exp_month">
                  有效期: {{ billingData.payment_method.exp_month }}/{{ billingData.payment_method.exp_year }}
                </div>
              </div>
            </div>
            <div v-else-if="billingData.is_new_account" class="no-payment new-account-hint">
              {{ paymentMethodHint }}
            </div>
            <div v-else class="no-payment">
              未绑定支付方式
            </div>
            
            <div class="invoice-link" v-if="billingData.invoice_url">
              <el-link type="primary" :href="billingData.invoice_url" target="_blank">
                <el-icon><Link /></el-icon> 查看最近发票
              </el-link>
            </div>

            <!-- 用户已要求不打开外部 Stripe Portal；帐单数据已在顶部 quick-summary 内嵌呈现 -->
          </div>
        </div>
      </div>
      
      <!-- 警告信息区域 -->
      <div class="alerts-container" v-if="billingData.failed_payment_message || billingData.top_up_error || isApproachingCacheLimit()">
         <el-alert
          v-if="billingData.failed_payment_message"
          :title="`支付失败: ${billingData.failed_payment_message}`"
          type="error"
          :closable="false"
          show-icon
          class="mb-10"
        />
        <el-alert
          v-if="billingData.top_up_error"
          :title="`充值错误: ${billingData.top_up_error}`"
          type="warning"
          :closable="false"
          show-icon
          class="mb-10"
        />
         <el-alert 
          v-if="isApproachingCacheLimit()" 
          :title="`注意：缓存使用率已达${getCacheUsagePercentage()}%`"
          type="warning"
          :closable="false"
          show-icon
        />
      </div>
      
      <!-- 错误信息 -->
      <el-alert
        v-if="!billingData.success"
        :title="billingData.error || '获取账单信息失败'"
        type="error"
        :closable="false"
        show-icon
      />

      <!-- Stripe Portal: 订阅明细 -->
      <div class="stripe-section" v-if="stripeSubscriptions.length > 0">
        <div class="section-title">
          <el-icon><Trophy /></el-icon>
          <span>订阅明细</span>
          <el-tag size="small" type="info" effect="plain">{{ stripeSubscriptions.length }}</el-tag>
        </div>
        <div class="sub-list">
          <div
            v-for="sub in stripeSubscriptions"
            :key="sub.id"
            class="sub-item"
          >
            <div class="sub-item-header">
              <div class="sub-desc">{{ sub.description || formatSubItem(sub) || '订阅' }}</div>
              <div class="sub-tags">
                <el-tag size="small" :type="subscriptionStatusType(sub.status)" effect="dark" round>
                  {{ formatSubscriptionStatus(sub.status) }}
                </el-tag>
                <el-tag v-if="sub.cancel_at_period_end" size="small" type="danger" effect="dark" round>
                  期末取消
                </el-tag>
              </div>
            </div>
            <div class="sub-item-body">
              <div class="sub-meta-row" v-if="sub.trial_end">
                <span class="meta-label">试用期截止</span>
                <span class="meta-value">{{ formatUnixTime(sub.trial_end) }}</span>
              </div>
              <div class="sub-meta-row" v-if="sub.current_period_end">
                <span class="meta-label">本期结束</span>
                <span class="meta-value">{{ formatUnixTime(sub.current_period_end) }}</span>
              </div>
              <div class="sub-meta-row" v-if="sub.cancel_at">
                <span class="meta-label">取消生效</span>
                <span class="meta-value">{{ formatUnixTime(sub.cancel_at) }}</span>
              </div>
              <div class="sub-meta-row" v-if="sub.id">
                <span class="meta-label">订阅 ID</span>
                <span class="meta-value mono">{{ sub.id }}</span>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Stripe Portal: 账单记录 -->
      <div class="stripe-section" v-if="stripeInvoices.length > 0">
        <div class="section-title">
          <el-icon><CreditCard /></el-icon>
          <span>账单记录</span>
          <el-tag size="small" type="info" effect="plain">{{ stripeInvoices.length }}</el-tag>
        </div>
        <div class="invoice-table">
          <div class="invoice-header">
            <div class="col-date">日期</div>
            <div class="col-amount">金额</div>
            <div class="col-status">状态</div>
            <div class="col-actions">操作</div>
          </div>
          <div
            v-for="inv in stripeInvoices"
            :key="inv.id"
            class="invoice-row"
          >
            <div class="col-date">{{ formatUnixTime(inv.effective_at || inv.finalized_at || inv.due_date) }}</div>
            <div class="col-amount mono">{{ formatCurrencyAmount(inv.amount_due, inv.currency) }}</div>
            <div class="col-status">
              <el-tag size="small" :type="invoiceStatusType(inv.status)" effect="plain">
                {{ formatInvoiceStatus(inv.status) }}
              </el-tag>
            </div>
            <div class="col-actions">
              <el-link
                v-if="inv.hosted_invoice_url"
                type="primary"
                size="small"
                :underline="false"
                @click="openInvoice(inv.hosted_invoice_url)"
              >
                查看
              </el-link>
            </div>
          </div>
        </div>
      </div>

      <!-- 客户账单地址（Stripe Portal） -->
      <div class="stripe-section" v-if="billingData.customer_name || billingData.customer_address">
        <div class="section-title">
          <el-icon><User /></el-icon>
          <span>账单联系信息</span>
        </div>
        <div class="customer-card">
          <div class="customer-row" v-if="billingData.customer_name">
            <span class="meta-label">姓名</span>
            <span class="meta-value">{{ billingData.customer_name }}</span>
          </div>
          <div class="customer-row" v-if="billingData.customer_address">
            <span class="meta-label">地址</span>
            <span class="meta-value">{{ formatAddress(billingData.customer_address) }}</span>
          </div>
        </div>
      </div>

      <!-- 原始数据（折叠） -->
      <el-collapse v-if="billingData.raw_data || billingData.stripe_portal" class="raw-data-collapse">
        <el-collapse-item v-if="billingData.raw_data" title="开发者原始数据">
          <pre class="raw-data">{{ JSON.stringify(billingData.raw_data, null, 2) }}</pre>
        </el-collapse-item>
        <el-collapse-item v-if="billingData.stripe_portal" title="Stripe Portal 原始响应">
          <pre class="raw-data">{{ JSON.stringify(billingData.stripe_portal, null, 2) }}</pre>
        </el-collapse-item>
      </el-collapse>
    </div>
    
    <template #footer>
      <div class="dialog-footer">
        <el-button @click="handleClose">关闭</el-button>
        <el-button type="primary" @click="copyToClipboard" v-if="billingData">
          <el-icon><CopyDocument /></el-icon> 复制数据
        </el-button>
      </div>
    </template>
  </el-dialog>
</template>

<script setup lang="ts">
import { ref, watch, computed } from 'vue';
import { ElMessage } from 'element-plus';
import { openUrl } from '@tauri-apps/plugin-opener';
import { 
  Loading, 
  Trophy, 
  Link, 
  User, 
  DataAnalysis, 
  CreditCard, 
  CopyDocument,
  Clock
} from '@element-plus/icons-vue';
import type { Account } from '@/types';

const props = defineProps<{
  modelValue: boolean;
  accountId: string;
  account?: Account;
  billingData?: any;
  loading?: boolean;
}>();

const emit = defineEmits<{
  'update:modelValue': [value: boolean];
  'refresh': [];
}>();

const visible = ref(props.modelValue);
const showFullResponse = ref(false);
const isDevinAccount = computed(() => props.account?.account_source === 'devin');
const paymentMethodHint = computed(() => isDevinAccount.value ? '银行卡信息仅试用账号未绑卡' : '未获取到支付方式');

watch(() => props.modelValue, (val) => {
  visible.value = val;
});

watch(visible, (val) => {
  emit('update:modelValue', val);
});

// 速览卡片：优先用后端 merged 后的 payment_method，其次源于 stripe_portal
const summaryCard = computed<{ last4?: string; brand?: string; exp_month?: number | string; exp_year?: number | string }>(() => {
  const pm = props.billingData?.payment_method;
  if (pm?.last4) {
    return {
      last4: pm.last4,
      brand: pm.type || pm.brand,
      exp_month: pm.exp_month,
      exp_year: pm.exp_year,
    };
  }
  const stripeCard = props.billingData?.stripe_portal?.customer?.default_payment_method?.card;
  if (stripeCard?.last4) {
    return {
      last4: stripeCard.last4,
      brand: stripeCard.brand,
      exp_month: stripeCard.exp_month,
      exp_year: stripeCard.exp_year,
    };
  }
  return {};
});

// 订阅名称 + 主题色代码的多源 fallback：
//   1. billingData.plan_name (后端 GetTeamBilling parser)
//   2. stripe_portal.subscriptions[0].items.data[0].price_details.product.name (付费 Devin 账号)
//   3. stripe_portal.subscriptions[0].description / plan.nickname
// 可限定枚举到 formatPlanName 表；CSS 主题色默认 free
const summaryPlanRaw = computed<string>(() => {
  const direct = (props.billingData?.plan_name || '').toString().trim();
  if (direct) return direct;
  const sub = props.billingData?.stripe_portal?.subscriptions?.data?.[0];
  const item = sub?.items?.data?.[0] || sub?.items?.[0];
  const productName = item?.price_details?.product?.name
    || item?.price?.product?.name
    || item?.price_details?.nickname
    || sub?.plan?.nickname;
  if (productName) return String(productName);
  if (sub?.description) return String(sub.description);
  return '';
});
const summaryPlanName = computed(() => {
  const raw = summaryPlanRaw.value;
  if (!raw) return '未知';
  const formatted = formatPlanName(raw);
  // formatPlanName 未命中枚举时会原样返回 raw（如 'Windsurf Pro'），这里保留。
  return formatted;
});
// CSS 类名仅取枚举关键字（pro/teams/enterprise/trial/free/starter/enterprise_self_serve）
const summaryPlanCode = computed(() => {
  const raw = summaryPlanRaw.value.toLowerCase();
  if (!raw) return 'free';
  if (raw.includes('enterprise_self_serve') || raw.includes('self-serve') || raw.includes('self serve')) return 'enterprise_self_serve';
  if (raw.includes('enterprise')) return 'enterprise';
  if (raw.includes('teams') || raw.includes('team')) return 'teams';
  if (raw.includes('trial')) return 'trial';
  if (raw.includes('starter')) return 'starter';
  if (raw.includes('pro')) return 'pro';
  return 'free';
});

// 选出主订阅（50%场景为未取消的 trialing/active）
const primarySubscription = computed<any | null>(() => {
  const subs = props.billingData?.stripe_portal?.subscriptions?.data;
  if (!Array.isArray(subs) || subs.length === 0) return null;
  return subs.find((s: any) => ['active', 'trialing'].includes(s.status)) || subs[0];
});

// 到期标签 + 值：优先级 cancel_at > trial_end > current_period_end > next_billing_date
const summaryExpiry = computed<{ label: string; value: string }>(() => {
  const data = props.billingData;
  const sub = primarySubscription.value;
  const fmtUnix = (u: number) => formatUnixTime(u);

  if (sub?.cancel_at) {
    return { label: '取消生效', value: fmtUnix(sub.cancel_at) };
  }
  if (data?.cancel_at_period_end && (sub?.current_period_end || data?.subscription_renewal_time)) {
    return {
      label: '期末取消',
      value: sub?.current_period_end ? fmtUnix(sub.current_period_end) : (data?.subscription_renewal_time || data?.next_billing_date || '-'),
    };
  }
  if (data?.on_trial && sub?.trial_end) {
    return { label: '试用期至', value: fmtUnix(sub.trial_end) };
  }
  if (data?.on_trial) {
    return { label: '试用期至', value: data?.subscription_renewal_time || data?.next_billing_date || '-' };
  }
  if (sub?.current_period_end) {
    return { label: '下次续费', value: fmtUnix(sub.current_period_end) };
  }
  if (data?.subscription_renewal_time) {
    return { label: '下次续费', value: data.subscription_renewal_time };
  }
  if (data?.next_billing_date) {
    return { label: '下次扣费', value: data.next_billing_date };
  }
  return { label: '到期时间', value: '-' };
});
const summaryExpiryLabel = computed(() => summaryExpiry.value.label);
const summaryExpiryValue = computed(() => summaryExpiry.value.value);

// Stripe Portal: 订阅明细列表（来自 fetch_stripe_portal_billing 的 subscriptions.data）
const stripeSubscriptions = computed<any[]>(() => {
  const subs = props.billingData?.stripe_portal?.subscriptions?.data;
  return Array.isArray(subs) ? subs : [];
});

// Stripe Portal: 账单记录（合并自 stripe_portal.invoices.data，或 MainLayout 提升后的 invoices）
const stripeInvoices = computed<any[]>(() => {
  const direct = props.billingData?.invoices;
  if (Array.isArray(direct)) return direct;
  const fromPortal = props.billingData?.stripe_portal?.invoices?.data;
  return Array.isArray(fromPortal) ? fromPortal : [];
});

// Unix 时间戳转 yyyy-MM-dd HH:mm
function formatUnixTime(unix?: number | null): string {
  if (!unix) return '-';
  try {
    const d = new Date(unix * 1000);
    const pad = (n: number) => n.toString().padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  } catch {
    return String(unix);
  }
}

// Stripe 金额按 currency 单位换算（最小货币单位 -> 主单位）
function formatCurrencyAmount(amount?: number | null, currency?: string | null): string {
  if (amount == null) return '-';
  const code = (currency || 'usd').toUpperCase();
  const symbol = code === 'USD' ? '$' : code === 'CNY' ? '¥' : code === 'EUR' ? '€' : '';
  return `${symbol}${(amount / 100).toFixed(2)} ${symbol ? '' : code}`.trim();
}

// 订阅描述兜底（从 items.price_details 拼出 "Windsurf Pro · 1×"）
function formatSubItem(sub: any): string {
  const items = sub?.items?.data || sub?.items;
  if (Array.isArray(items) && items.length > 0) {
    const first = items[0];
    const name = first?.price_details?.product?.name || first?.price_details?.id || '';
    const qty = first?.quantity ? `${first.quantity}×` : '';
    return [name, qty].filter(Boolean).join(' · ');
  }
  return '';
}

function formatSubscriptionStatus(status?: string): string {
  switch (status) {
    case 'active': return '活跃';
    case 'trialing': return '试用中';
    case 'past_due': return '逾期';
    case 'canceled': return '已取消';
    case 'unpaid': return '未支付';
    case 'incomplete': return '未完成';
    case 'incomplete_expired': return '已过期';
    case 'paused': return '已暂停';
    default: return status || '-';
  }
}

function subscriptionStatusType(status?: string): 'success' | 'warning' | 'danger' | 'info' | '' {
  switch (status) {
    case 'active': return 'success';
    case 'trialing': return 'warning';
    case 'past_due':
    case 'unpaid':
    case 'canceled':
    case 'incomplete_expired': return 'danger';
    case 'paused':
    case 'incomplete': return 'info';
    default: return '';
  }
}

function formatInvoiceStatus(status?: string): string {
  switch (status) {
    case 'paid': return '已付';
    case 'open': return '待支付';
    case 'draft': return '草稿';
    case 'uncollectible': return '无法收取';
    case 'void': return '已作废';
    default: return status || '-';
  }
}

function invoiceStatusType(status?: string): 'success' | 'warning' | 'danger' | 'info' | '' {
  switch (status) {
    case 'paid': return 'success';
    case 'open': return 'warning';
    case 'uncollectible': return 'danger';
    case 'void':
    case 'draft': return 'info';
    default: return '';
  }
}

function formatAddress(addr: any): string {
  if (!addr) return '-';
  const parts = [addr.line1, addr.line2, addr.city, addr.state, addr.postal_code, addr.country]
    .filter((s: any) => !!s);
  return parts.join(', ');
}

async function openInvoice(url: string) {
  try {
    await openUrl(url);
  } catch (err) {
    ElMessage.error(`打开账单失败: ${err}`);
  }
}

// 配额百分比
const quotaPercentage = computed(() => {
  if (!props.billingData?.total_quota || !props.billingData?.used_quota) return 0;
  return Math.min(Math.round((props.billingData.used_quota / props.billingData.total_quota) * 100), 100);
});

// 配额颜色
const quotaColor = computed(() => {
  const percentage = quotaPercentage.value;
  if (percentage < 50) return '#10b981';
  if (percentage < 80) return '#f59e0b';
  return '#ef4444';
});

// 缓存使用率百分比
function getCacheUsagePercentage() {
  if (!props.billingData?.cache_limit || props.billingData?.used_quota === undefined) return 0;
  const percentage = Math.round((props.billingData.used_quota / props.billingData.cache_limit) * 100);
  return Math.min(percentage, 100);
}

// 缓存使用率标签类型
function getCacheUsageType() {
  const percentage = getCacheUsagePercentage();
  if (percentage < 50) return 'success';
  if (percentage < 80) return 'warning';
  return 'danger';
}

// 是否接近缓存限制
function isApproachingCacheLimit() {
  const percentage = getCacheUsagePercentage();
  return percentage >= 80;
}

// 格式化配额
function formatQuota(num: number | undefined | null) {
  if (!num) return '0.00';
  return (num / 100).toFixed(2);
}

// 计算席位使用百分比
function getSeatUsagePercentage() {
  if (!props.billingData?.num_seats || !props.billingData?.num_users) return 0;
  return Math.min(Math.round((props.billingData.num_users / props.billingData.num_seats) * 100), 100);
}

// 获取席位使用颜色
function getSeatUsageColor() {
  const percentage = getSeatUsagePercentage();
  if (percentage < 50) return '#10b981';
  if (percentage < 80) return '#f59e0b';
  if (percentage >= 100) return '#ef4444';
  return '#ef4444';
}

// 格式化支付方式
function formatPaymentType(type: string) {
  const types: Record<string, string> = {
    'unionpay': '银联卡',
    'card': '信用卡',
    'visa': 'Visa',
    'mastercard': 'MasterCard',
    'amex': 'American Express',
    'jcb': 'JCB',
    'diners': 'Diners Club',
    'discover': 'Discover',
    'alipay': '支付宝',
    'wechat': '微信支付'
  };
  return types[type?.toLowerCase()] || type || '未知';
}

// 格式化套餐名称（覆盖完整 TeamsTier 枚举，与后端 proto_parser 同表）
function formatPlanName(name: string) {
  const names: Record<string, string> = {
    'pro': 'Pro 专业版',
    'pro_ultimate': 'Pro Ultimate',
    'teams': 'Teams 团队版',
    'teams_ultimate': 'Teams Ultimate',
    'enterprise': 'Enterprise 企业版',
    'enterprise_saas': 'Enterprise SaaS',
    'enterprise_self_serve': 'Enterprise 企业自助版',
    'enterprise_self_hosted': 'Enterprise 自建版',
    'enterprise_saas_pooled': 'Enterprise SaaS Pooled',
    'hybrid': 'Hybrid 混合版',
    'waitlist_pro': 'Pro 候补',
    'trial': 'Trial 试用版',
    'free': 'Free 免费版',
    'starter': 'Starter 入门版',
    'max': 'Max',
    'devin_enterprise': 'Devin Enterprise',
    'devin_teams': 'Devin Teams',
    'devin_teams_v2': 'Devin Teams v2',
    'devin_pro': 'Devin Pro',
    'devin_max': 'Devin Max',
    'devin_free': 'Devin Free',
    'devin_trial': 'Devin Trial 试用版',
  };
  return names[name?.toLowerCase()] || name || '未知';
}

function handleClose() {
  visible.value = false;
  showFullResponse.value = false;
}

async function copyToClipboard() {
  if (props.billingData) {
    try {
      await navigator.clipboard.writeText(JSON.stringify(props.billingData, null, 2));
      ElMessage.success('已复制到剪贴板');
    } catch (error) {
      ElMessage.error('复制失败');
    }
  }
}
</script>

<style scoped lang="scss">
.billing-dialog {
  :deep(.el-dialog__body) {
    padding: 20px 24px;
  }
}

.loading-container {
  text-align: center;
  padding: 60px 0;
  color: #909399;
  
  p {
    margin-top: 16px;
  }
}

.billing-content {
  display: flex;
  flex-direction: column;
  gap: 24px;
}

/* 速览面板：卡号 + 订阅类型 + 到期 */
.quick-summary {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 12px;
  padding: 16px;
  background: linear-gradient(135deg, #f0f9ff 0%, #faf5ff 100%);
  border: 1px solid #e0e7ff;
  border-radius: 12px;

  .qs-item {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    background: #fff;
    border-radius: 10px;
    padding: 12px;
    transition: box-shadow 0.2s ease;

    &:hover {
      box-shadow: 0 2px 8px rgba(99, 102, 241, 0.08);
    }
  }

  .qs-icon {
    width: 32px;
    height: 32px;
    border-radius: 8px;
    background: #eef2ff;
    color: #6366f1;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 16px;
    flex-shrink: 0;
  }

  .qs-card .qs-icon {
    background: #ecfdf5;
    color: #10b981;
  }

  .qs-plan .qs-icon {
    background: #fef3c7;
    color: #f59e0b;
  }

  .qs-expiry .qs-icon {
    background: #fee2e2;
    color: #ef4444;
  }

  .qs-body {
    min-width: 0;
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .qs-label {
    font-size: 11px;
    color: #94a3b8;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    font-weight: 500;
  }

  .qs-value {
    font-size: 14px;
    color: #0f172a;
    font-weight: 600;
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    line-height: 1.4;

    &.mono {
      font-family: 'Roboto Mono', 'Cascadia Code', monospace;
    }

    &.qs-empty {
      color: #94a3b8;
      font-weight: 400;
    }
  }

  .qs-brand {
    font-size: 12px;
    color: #64748b;
    font-weight: 500;
  }

  .qs-mask {
    color: #0f172a;
  }

  .qs-exp {
    font-size: 11px;
    color: #94a3b8;
    font-weight: 400;
  }

  .qs-plan-name {
    color: #0f172a;
  }
}

/* 顶部订阅卡片 */
.subscription-card {
  position: relative;
  padding: 24px;
  border-radius: 16px;
  color: white;
  overflow: hidden;
  box-shadow: 0 8px 20px rgba(0, 0, 0, 0.1);
  transition: transform 0.3s ease;

  &:hover {
    transform: translateY(-2px);
  }

  .card-bg-icon {
    position: absolute;
    right: -20px;
    top: -20px;
    font-size: 180px;
    opacity: 0.1;
    transform: rotate(15deg);
  }

  /* 不同套餐的主题色 */
  &.plan-pro {
    background: linear-gradient(135deg, #3b82f6 0%, #2563eb 100%);
  }
  &.plan-teams {
    background: linear-gradient(135deg, #10b981 0%, #059669 100%);
  }
  &.plan-enterprise {
    background: linear-gradient(135deg, #8b5cf6 0%, #7c3aed 100%);
  }
  &.plan-free {
    background: linear-gradient(135deg, #9ca3af 0%, #6b7280 100%);
  }
  &.plan-trial {
    background: linear-gradient(135deg, #f59e0b 0%, #d97706 100%);
  }
  &.plan-enterprise_self_serve {
    background: linear-gradient(135deg, #a855f7 0%, #9333ea 100%);
  }

  .sub-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 24px;
    position: relative;
    z-index: 1;

    .plan-info {
      .plan-name {
        font-size: 24px;
        font-weight: 700;
        display: flex;
        align-items: center;
        gap: 8px;
        margin-bottom: 8px;
      }
      
      .plan-status {
        display: flex;
        gap: 6px;
      }
    }

    .sub-price {
      text-align: right;
      .currency { font-size: 20px; opacity: 0.9; }
      .amount { font-size: 32px; font-weight: 700; }
      .unit { font-size: 14px; opacity: 0.8; }
    }
  }

  .sub-dates {
    display: flex;
    gap: 32px;
    position: relative;
    z-index: 1;
    padding-top: 16px;
    border-top: 1px solid rgba(255, 255, 255, 0.2);

    .date-item {
      display: flex;
      flex-direction: column;
      gap: 4px;
      
      .label {
        font-size: 12px;
        opacity: 0.8;
        text-transform: uppercase;
        letter-spacing: 0.5px;
      }
      
      .value {
        font-size: 14px;
        font-weight: 500;
        font-family: 'Roboto Mono', monospace;
      }
    }
  }
}

/* 信息网格 */
.info-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 20px;
}

.info-card {
  background: #fff;
  border: 1px solid #e4e7ed;
  border-radius: 12px;
  padding: 20px;
  display: flex;
  flex-direction: column;
  transition: all 0.3s ease;

  &:hover {
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.05);
    border-color: #dcdfe6;
  }

  .card-title {
    font-size: 15px;
    font-weight: 600;
    color: #303133;
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 16px;
    
    .el-icon {
      color: #909399;
    }
  }

  .card-content {
    flex: 1;
  }
}

/* 席位卡片 */
.seats-card {
  .usage-circle-container {
    display: flex;
    justify-content: center;
    margin-bottom: 16px;
    position: relative;
    
    .percentage-value {
      font-size: 24px;
      font-weight: 700;
      color: #303133;
    }
    
    .percentage-label {
      font-size: 12px;
      color: #909399;
    }
  }

  .usage-details {
    display: flex;
    flex-direction: column;
    gap: 8px;
    
    .detail-row {
      display: flex;
      justify-content: space-between;
      font-size: 13px;
      
      .label { color: #606266; }
      .value { font-family: 'Roboto Mono', monospace; font-weight: 500; }
    }
  }
}

/* 配额卡片 */
.quota-card {
  .quota-main {
    margin-bottom: 16px;
    
    .quota-text {
      display: flex;
      align-items: baseline;
      gap: 4px;
      margin-bottom: 8px;
      
      .current { font-size: 20px; font-weight: 700; color: #303133; }
      .divider { font-size: 14px; color: #909399; }
      .total { font-size: 14px; color: #606266; }
    }
  }
  
  .quota-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-bottom: 16px;
  }
  
  .cache-info {
    background: #f5f7fa;
    border-radius: 8px;
    padding: 12px;
    
    .cache-header {
      display: flex;
      justify-content: space-between;
      font-size: 12px;
      color: #606266;
      margin-bottom: 6px;
    }
  }
}

/* 支付卡片 */
.payment-card {
  .payment-method {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 16px;
    
    .method-icon {
      width: 40px;
      height: 40px;
      background: #f0f2f5;
      border-radius: 8px;
      display: flex;
      align-items: center;
      justify-content: center;
      font-size: 20px;
      color: #606266;
    }
    
    .method-info {
      flex: 1;
      
      .method-type { font-weight: 600; color: #303133; font-size: 14px; }
      .method-number { font-family: 'Roboto Mono', monospace; color: #606266; font-size: 13px; }
      .method-exp { font-size: 12px; color: #909399; margin-top: 2px; }
    }
  }
  
  .no-payment {
    text-align: center;
    padding: 20px 0;
    color: #909399;
    font-size: 13px;
    background: #f5f7fa;
    border-radius: 8px;
    margin-bottom: 16px;
  }
  
  .invoice-link {
    text-align: center;
    border-top: 1px solid #f0f2f5;
    padding-top: 12px;
  }

  .portal-link {
    text-align: center;
    border-top: 1px solid #f0f2f5;
    padding-top: 12px;
    margin-top: 8px;

    .el-button {
      width: 100%;
    }
  }

  .new-account-hint {
    background: #fef6ec !important;
    color: #e6a23c !important;
    font-size: 12px;
    line-height: 1.6;
  }
}

/* Stripe Portal 数据区域 */
.stripe-section {
  background: #fff;
  border: 1px solid #e4e7ed;
  border-radius: 12px;
  padding: 16px 20px;

  .section-title {
    font-size: 14px;
    font-weight: 600;
    color: #303133;
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 12px;

    .el-icon {
      color: #909399;
    }
  }
}

.sub-list {
  display: flex;
  flex-direction: column;
  gap: 12px;

  .sub-item {
    background: #f8f9fc;
    border: 1px solid #e8eaef;
    border-radius: 10px;
    padding: 12px 14px;

    .sub-item-header {
      display: flex;
      justify-content: space-between;
      align-items: flex-start;
      gap: 10px;
      margin-bottom: 10px;

      .sub-desc {
        font-size: 14px;
        font-weight: 600;
        color: #303133;
      }

      .sub-tags {
        display: flex;
        gap: 6px;
        flex-shrink: 0;
      }
    }

    .sub-item-body {
      display: grid;
      grid-template-columns: repeat(2, 1fr);
      gap: 6px 16px;

      .sub-meta-row {
        display: flex;
        justify-content: space-between;
        font-size: 12px;

        .meta-label { color: #909399; }
        .meta-value { color: #303133; font-weight: 500; }
        .meta-value.mono { font-family: 'Roboto Mono', monospace; font-size: 11px; }
      }
    }
  }
}

.invoice-table {
  border: 1px solid #ebeef5;
  border-radius: 8px;
  overflow: hidden;

  .invoice-header,
  .invoice-row {
    display: grid;
    grid-template-columns: 2fr 1fr 1fr 0.7fr;
    align-items: center;
    padding: 10px 14px;
    font-size: 13px;
  }

  .invoice-header {
    background: #f5f7fa;
    color: #606266;
    font-weight: 600;
    font-size: 12px;
  }

  .invoice-row {
    border-top: 1px solid #f0f2f5;
    color: #303133;

    &:hover { background: #fafbfc; }

    .col-amount.mono { font-family: 'Roboto Mono', monospace; }
  }
}

.customer-card {
  display: flex;
  flex-direction: column;
  gap: 6px;

  .customer-row {
    display: flex;
    gap: 12px;
    font-size: 13px;

    .meta-label { color: #909399; min-width: 50px; }
    .meta-value { color: #303133; }
  }
}

/* 警告和原始数据 */
.alerts-container {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.mb-10 {
  margin-bottom: 10px;
}

.raw-data-collapse {
  border: 1px solid #e4e7ed;
  border-radius: 8px;
  overflow: hidden;
  
  :deep(.el-collapse-item__header) {
    padding: 0 16px;
    background: #f8f9fa;
  }
  
  :deep(.el-collapse-item__content) {
    padding: 0;
  }
}

.raw-data {
  margin: 0;
  padding: 16px;
  background: #282c34;
  color: #abb2bf;
  font-size: 12px;
  font-family: 'Roboto Mono', monospace;
  overflow-x: auto;
}

.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 12px;
}

/* 响应式适配 */
@media (max-width: 768px) {
  .info-grid {
    grid-template-columns: 1fr;
  }
  
  .subscription-card .sub-header {
    flex-direction: column;
    gap: 16px;
    
    .sub-price {
      text-align: left;
    }
  }
}

/* 深色模式适配 */
:root.dark {
  .subscription-card.plan-pro { background: linear-gradient(135deg, #1e40af 0%, #1d4ed8 100%); }
  .subscription-card.plan-teams { background: linear-gradient(135deg, #065f46 0%, #059669 100%); }
  .subscription-card.plan-enterprise { background: linear-gradient(135deg, #5b21b6 0%, #7c3aed 100%); }
  .subscription-card.plan-trial { background: linear-gradient(135deg, #b45309 0%, #d97706 100%); }
  .subscription-card.plan-enterprise_self_serve { background: linear-gradient(135deg, #7e22ce 0%, #9333ea 100%); }
  
  .info-card {
    background: #1d1e1f;
    border-color: #4c4d4f;
    
    .card-title { color: #e5eaf3; }
    
    .usage-circle-container .percentage-value { color: #e5eaf3; }
    .detail-row .label { color: #a3a6ad; }
    .detail-row .value { color: #cfd3dc; }
    
    .quota-text .current { color: #e5eaf3; }
    .quota-text .total { color: #a3a6ad; }
    
    .cache-info { background: #262729; }
    
    .payment-method .method-icon { background: #262729; color: #a3a6ad; }
    .method-type { color: #e5eaf3; }
    .method-number { color: #cfd3dc; }
    
    .no-payment { background: #262729; }
    .invoice-link { border-top-color: #4c4d4f; }
  }
  
  .raw-data-collapse {
    border-color: #4c4d4f;
    :deep(.el-collapse-item__header) {
      background: #262729;
      color: #e5eaf3;
    }
  }
}
</style>
