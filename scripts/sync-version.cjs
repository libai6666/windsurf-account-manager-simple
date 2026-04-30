/**
 * 版本号同步脚本
 * 从 src-tauri/tauri.conf.json 读取版本号，同步到其他需要的文件
 * 
 * 使用方法：node scripts/sync-version.js
 */

const fs = require('fs');
const path = require('path');

const ROOT_DIR = path.join(__dirname, '..');

// 读取 tauri.conf.json 中的版本号
function getVersion() {
  const tauriConfPath = path.join(ROOT_DIR, 'src-tauri', 'tauri.conf.json');
  const content = JSON.parse(fs.readFileSync(tauriConfPath, 'utf-8'));
  return content.version;
}

// 更新 package.json
function updatePackageJson(version) {
  const filePath = path.join(ROOT_DIR, 'package.json');
  const content = JSON.parse(fs.readFileSync(filePath, 'utf-8'));
  if (content.version !== version) {
    content.version = version;
    fs.writeFileSync(filePath, JSON.stringify(content, null, 2) + '\n');
    console.log(`✅ package.json: ${version}`);
    return true;
  }
  console.log(`⏭️  package.json: 已是最新 (${version})`);
  return false;
}

function updatePackageLockJson(version) {
  const filePath = path.join(ROOT_DIR, 'package-lock.json');
  if (!fs.existsSync(filePath)) {
    console.log(`⏭️  package-lock.json: 文件不存在，跳过`);
    return false;
  }
  const content = JSON.parse(fs.readFileSync(filePath, 'utf-8'));
  let changed = false;
  if (content.version !== version) {
    content.version = version;
    changed = true;
  }
  if (content.packages?.['']?.version !== version) {
    content.packages[''].version = version;
    changed = true;
  }
  if (changed) {
    fs.writeFileSync(filePath, JSON.stringify(content, null, 2) + '\n');
    console.log(`✅ package-lock.json: ${version}`);
    return true;
  }
  console.log(`⏭️  package-lock.json: 已是最新 (${version})`);
  return false;
}

// 更新 Cargo.toml
function updateCargoToml(version) {
  const filePath = path.join(ROOT_DIR, 'src-tauri', 'Cargo.toml');
  let content = fs.readFileSync(filePath, 'utf-8');
  const regex = /^version\s*=\s*"[^"]+"/m;
  const newContent = content.replace(regex, `version = "${version}"`);
  if (content !== newContent) {
    fs.writeFileSync(filePath, newContent);
    console.log(`✅ Cargo.toml: ${version}`);
    return true;
  }
  console.log(`⏭️  Cargo.toml: 已是最新 (${version})`);
  return false;
}

// 更新 Windows manifest 文件
function updateManifest(version) {
  const filePath = path.join(ROOT_DIR, 'src-tauri', 'windsurf-account-manager.exe.manifest');
  if (!fs.existsSync(filePath)) {
    console.log(`⏭️  exe.manifest: 文件不存在，跳过`);
    return false;
  }
  let content = fs.readFileSync(filePath, 'utf-8');
  const versionWithBuild = `${version}.0`;
  const regex = /(<assemblyIdentity[\s\S]*?version=")[\d.]+("[\s\S]*?name="com\.chao\.windsurf-account-manager")/;
  const newContent = content.replace(regex, `$1${versionWithBuild}$2`);
  if (content !== newContent) {
    fs.writeFileSync(filePath, newContent);
    console.log(`✅ exe.manifest: ${versionWithBuild}`);
    return true;
  }
  console.log(`⏭️  exe.manifest: 已是最新 (${versionWithBuild})`);
  return false;
}

// 主函数
function main() {
  console.log('🔄 正在同步版本号...\n');
  
  const version = getVersion();
  console.log(`📦 tauri.conf.json 版本号: ${version}\n`);
  
  let updated = 0;
  if (updatePackageJson(version)) updated++;
  if (updatePackageLockJson(version)) updated++;
  if (updateCargoToml(version)) updated++;
  if (updateManifest(version)) updated++;
  
  console.log(`\n✨ 完成！更新了 ${updated} 个文件`);
  console.log('\n💡 提示: build.rs 会在构建时自动从 tauri.conf.json 读取版本号');
  console.log('💡 提示: 前端代码会在运行时从后端 API 获取版本号');
}

main();
