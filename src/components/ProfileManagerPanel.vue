<template>
  <div class="profile-manager-panel">
    <div class="profile-hero">
      <div>
        <div class="eyebrow">Windsurf Profiles</div>
        <h1>分身管理</h1>
        <p>为每个编辑器窗口分配独立的 user-data-dir，实现账号、状态和机器码文件隔离。</p>
      </div>
      <div class="hero-actions">
        <el-button :icon="Refresh" :loading="loading" @click="loadProfiles">刷新</el-button>
        <el-button type="primary" :icon="Plus" @click="openCreateDialog">新建分身</el-button>
      </div>
    </div>

    <el-alert
      title="主实例仍使用默认 Windsurf 数据目录；分身窗口会通过 --user-data-dir 独立启动。分身首次登录建议先选择手动目标号并点击“切到目标号”，避免浏览器 windsurf:// 协议被主实例接管。"
      type="info"
      :closable="false"
      show-icon
      class="profile-alert"
    />

    <div v-if="loading" class="profile-loading">
      <el-icon class="is-loading" size="32"><Loading /></el-icon>
    </div>

    <div v-else class="profile-list">
      <div class="profile-grid">
        <el-card
          v-for="item in visibleProfiles"
          :key="item.profile.id"
          class="profile-card"
          shadow="hover"
          :class="{ 'main-profile': item.profile.id === MAIN_PROFILE_ID }"
        >
        <template #header>
          <div class="profile-card-header">
            <div class="profile-title-block">
              <div class="profile-name-row">
                <span class="profile-name">{{ item.profile.name }}</span>
                <el-tag v-if="item.profile.id === MAIN_PROFILE_ID" size="small" type="info">主实例</el-tag>
                <el-tag v-else size="small" type="success">分身</el-tag>
              </div>
              <div class="profile-path" :title="item.profile.userDataDir">{{ item.profile.userDataDir }}</div>
            </div>
            <el-tag :type="item.isRunning ? 'success' : 'info'" effect="dark">
              {{ item.isRunning ? '运行中' : '未运行' }}
            </el-tag>
          </div>
        </template>

        <div class="profile-status">
          <div class="status-item">
            <span class="label">实际登录账号</span>
            <strong>{{ item.currentInfo?.email || '未检测到' }}</strong>
          </div>
          <div class="status-item">
            <span class="label">套餐</span>
            <strong>{{ item.currentInfo?.plan_name || '-' }}</strong>
          </div>
          <div class="status-item">
            <span class="label">{{ item.profile.id === MAIN_PROFILE_ID ? '自动换号' : '手动目标号' }}</span>
            <strong v-if="item.profile.id === MAIN_PROFILE_ID">{{ settingsStore.settings.autoSwitchEnabled ? '已开启' : '未开启' }}</strong>
            <strong v-else>{{ boundAccountEmail(item.profile.boundAccountId) }}</strong>
          </div>
        </div>

        <div v-if="item.profile.id === MAIN_PROFILE_ID" class="profile-config">
          <el-form label-width="96px" size="small">
            <el-form-item label="自动换号">
              <el-switch
                :model-value="settingsStore.settings.autoSwitchEnabled"
                active-text="开启"
                inactive-text="关闭"
                :disabled="!settingsStore.settings.seamlessSwitchEnabled"
                @change="updateMainAutoSwitchSettings({ autoSwitchEnabled: Boolean($event) })"
              />
              <div v-if="!settingsStore.settings.seamlessSwitchEnabled" class="form-tip">
                需要先在设置中启用无感换号
              </div>
            </el-form-item>
            <el-form-item v-if="settingsStore.settings.autoSwitchEnabled" label="换号分组">
              <el-select
                :model-value="settingsStore.settings.autoSwitchGroup"
                placeholder="选择分组"
                @change="handleMainAutoSwitchGroupChange(String($event))"
              >
                <el-option
                  v-for="group in settingsStore.groups"
                  :key="group"
                  :label="group"
                  :value="group"
                />
              </el-select>
              <div class="form-tip">主实例会从该分组中选择可用账号自动切换</div>
            </el-form-item>
            <el-form-item v-if="settingsStore.settings.autoSwitchEnabled" label="手动目标号">
              <el-select
                :model-value="settingsStore.settings.autoSwitchCurrentAccountId || ''"
                filterable
                clearable
                placeholder="选择要手动切到的账号"
                @change="updateMainAutoSwitchSettings({ autoSwitchCurrentAccountId: String($event || '') || null })"
              >
                <el-option
                  v-for="account in mainGroupAccounts"
                  :key="account.id"
                  :label="accountOptionLabel(account)"
                  :value="account.id"
                />
              </el-select>
              <div class="form-tip">仅作为手动切号目标；自动换号始终读取上方实际登录账号判断</div>
            </el-form-item>
            <el-form-item v-if="settingsStore.settings.autoSwitchEnabled" label="阈值">
              <el-input-number
                :model-value="settingsStore.settings.autoSwitchThreshold"
                :min="0"
                :max="99"
                :step="1"
                @change="updateMainAutoSwitchSettings({ autoSwitchThreshold: Number($event ?? 10) })"
              />
            </el-form-item>
            <el-form-item v-if="settingsStore.settings.autoSwitchEnabled" label="检测间隔">
              <el-select
                :model-value="settingsStore.settings.autoSwitchCheckInterval"
                @change="updateMainAutoSwitchSettings({ autoSwitchCheckInterval: Number($event) })"
              >
                <el-option label="10 秒" :value="10" />
                <el-option label="30 秒" :value="30" />
                <el-option label="1 分钟" :value="60" />
                <el-option label="3 分钟" :value="180" />
                <el-option label="5 分钟" :value="300" />
                <el-option label="10 分钟" :value="600" />
                <el-option label="15 分钟" :value="900" />
              </el-select>
            </el-form-item>
          </el-form>
        </div>

        <div v-else class="profile-config">
          <el-form label-width="96px" size="small">
            <el-form-item label="自动换号">
              <el-switch
                :model-value="item.profile.autoSwitch.enabled"
                @change="updateAutoSwitch(item.profile.id, Boolean($event), item.profile.autoSwitch.group, item.profile.autoSwitch.threshold, item.profile.autoSwitch.checkInterval)"
              />
            </el-form-item>
            <el-form-item label="换号分组">
              <el-select
                :model-value="item.profile.autoSwitch.group"
                @change="handleAutoSwitchGroupChange(item, String($event))"
              >
                <el-option
                  v-for="group in settingsStore.groups"
                  :key="group"
                  :label="group"
                  :value="group"
                />
              </el-select>
            </el-form-item>
            <el-form-item label="手动目标号">
              <el-select
                :model-value="item.profile.boundAccountId || ''"
                filterable
                clearable
                placeholder="选择要手动切到的账号"
                @change="handleBindAccount(item.profile.id, String($event || ''))"
              >
                <el-option
                  v-for="account in profileGroupAccounts(item.profile)"
                  :key="account.id"
                  :label="accountOptionLabel(account)"
                  :value="account.id"
                />
              </el-select>
              <div class="form-tip">仅作为手动切号目标；自动换号会根据实际登录账号和分组配额判断</div>
            </el-form-item>
            <el-form-item label="阈值">
              <el-input-number
                :model-value="item.profile.autoSwitch.threshold"
                :min="0"
                :max="100"
                @change="updateAutoSwitch(item.profile.id, item.profile.autoSwitch.enabled, item.profile.autoSwitch.group, Number($event ?? 10), item.profile.autoSwitch.checkInterval)"
              />
            </el-form-item>
            <el-form-item label="检测间隔">
              <el-select
                :model-value="item.profile.autoSwitch.checkInterval || 300"
                @change="updateAutoSwitch(item.profile.id, item.profile.autoSwitch.enabled, item.profile.autoSwitch.group, item.profile.autoSwitch.threshold, Number($event || 300))"
              >
                <el-option label="10 秒" :value="10" />
                <el-option label="30 秒" :value="30" />
                <el-option label="1 分钟" :value="60" />
                <el-option label="3 分钟" :value="180" />
                <el-option label="5 分钟" :value="300" />
                <el-option label="10 分钟" :value="600" />
                <el-option label="15 分钟" :value="900" />
              </el-select>
            </el-form-item>
          </el-form>
        </div>

        <div class="profile-actions">
          <el-button
            :type="item.isRunning ? 'danger' : 'default'"
            :icon="item.isRunning ? Close : VideoPlay"
            :loading="actionLoading === `launch:${item.profile.id}` || actionLoading === `stop:${item.profile.id}`"
            @click="item.isRunning ? handleStop(item.profile.id) : handleLaunch(item.profile.id)"
          >
            {{ item.isRunning ? '关闭' : '启动' }}
          </el-button>
          <el-button
            type="primary"
            :icon="Switch"
            :disabled="item.profile.id === MAIN_PROFILE_ID ? !settingsStore.settings.autoSwitchCurrentAccountId : !item.profile.boundAccountId"
            :loading="actionLoading === `switch:${item.profile.id}`"
            @click="item.profile.id === MAIN_PROFILE_ID ? handleMainSwitch(settingsStore.settings.autoSwitchCurrentAccountId) : handleSwitch(item.profile.id, item.profile.boundAccountId)"
          >
            切到目标号
          </el-button>
          <el-button
            type="warning"
            :icon="RefreshRight"
            :loading="actionLoading === `auto:${item.profile.id}`"
            :disabled="item.profile.id === MAIN_PROFILE_ID && (!settingsStore.settings.autoSwitchEnabled || !settingsStore.settings.seamlessSwitchEnabled)"
            @click="item.profile.id === MAIN_PROFILE_ID ? handleMainCheckAutoSwitch() : handleCheckAutoSwitch(item.profile.id)"
          >
            自动检测
          </el-button>
          <el-dropdown v-if="item.profile.id !== MAIN_PROFILE_ID" trigger="click">
            <el-button :icon="MoreFilled" />
            <template #dropdown>
              <el-dropdown-menu>
                <el-dropdown-item @click="openRenameDialog(item.profile)">重命名</el-dropdown-item>
                <el-dropdown-item divided @click="handleDelete(item.profile.id)">删除分身</el-dropdown-item>
              </el-dropdown-menu>
            </template>
          </el-dropdown>
        </div>
        </el-card>
      </div>

      <div class="profile-pagination">
        <el-pagination
          v-model:current-page="currentProfilePage"
          background
          layout="prev, pager, next, jumper, total"
          :page-size="PROFILE_PAGE_SIZE"
          :total="profiles.length"
        />
      </div>
    </div>

    <el-dialog v-model="createDialogVisible" title="新建 Windsurf 分身" width="420px">
      <el-form label-width="88px">
        <el-form-item label="分身名称">
          <el-input v-model="profileNameInput" placeholder="例如：工作号 A" @keyup.enter="handleCreate" />
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="createDialogVisible = false">取消</el-button>
        <el-button type="primary" :loading="actionLoading === 'create'" @click="handleCreate">创建</el-button>
      </template>
    </el-dialog>

    <el-dialog v-model="renameDialogVisible" title="重命名分身" width="420px">
      <el-form label-width="88px">
        <el-form-item label="分身名称">
          <el-input v-model="profileNameInput" @keyup.enter="handleRename" />
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="renameDialogVisible = false">取消</el-button>
        <el-button type="primary" :loading="actionLoading === 'rename'" @click="handleRename">保存</el-button>
      </template>
    </el-dialog>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { Close, MoreFilled, Plus, Refresh, RefreshRight, Loading, Switch, VideoPlay } from '@element-plus/icons-vue';
import { apiService, profileApi } from '@/api';
import { useAccountsStore, useSettingsStore } from '@/store';
import type { Account, ProfileRuntimeInfo, Settings, WindsurfProfile } from '@/types';

const MAIN_PROFILE_ID = 'main';
const PROFILE_PAGE_SIZE = 4;
const accountsStore = useAccountsStore();
const settingsStore = useSettingsStore();
const profiles = ref<ProfileRuntimeInfo[]>([]);
const currentProfilePage = ref(1);
const loading = ref(false);
const actionLoading = ref('');
const createDialogVisible = ref(false);
const renameDialogVisible = ref(false);
const profileNameInput = ref('');
const editingProfileId = ref('');
let profileStatusTimer: ReturnType<typeof setInterval> | null = null;

const accountEmailMap = computed(() => {
  const map = new Map<string, string>();
  for (const account of accountsStore.accounts) {
    map.set(account.id, account.email);
  }
  return map;
});

function boundAccountEmail(accountId?: string | null) {
  if (!accountId) return '未绑定';
  return accountEmailMap.value.get(accountId) || accountId;
}

function profileGroupAccounts(profile: WindsurfProfile) {
  const group = profile.autoSwitch.group;
  if (!group) return [];
  return accountsStore.accounts.filter(account => account.group === group);
}

const mainGroupAccounts = computed(() => {
  const group = settingsStore.settings.autoSwitchGroup;
  if (!group) return [];
  return accountsStore.accounts.filter(account => account.group === group);
});

const profilePageCount = computed(() => Math.max(1, Math.ceil(profiles.value.length / PROFILE_PAGE_SIZE)));

const visibleProfiles = computed(() => {
  const start = (currentProfilePage.value - 1) * PROFILE_PAGE_SIZE;
  return profiles.value.slice(start, start + PROFILE_PAGE_SIZE);
});

function normalizeProfilePage() {
  if (currentProfilePage.value > profilePageCount.value) {
    currentProfilePage.value = profilePageCount.value;
  }
  if (currentProfilePage.value < 1) {
    currentProfilePage.value = 1;
  }
}

function accountOptionLabel(account: Account) {
  const daily = account.daily_quota_remaining;
  const weekly = account.weekly_quota_remaining;
  if (daily === undefined && weekly === undefined) return account.email;
  return `${account.email} (日${daily ?? '?'}%/周${weekly ?? '?'}%)`;
}

function upsertProfileRuntime(updated: ProfileRuntimeInfo) {
  const index = profiles.value.findIndex(item => item.profile.id === updated.profile.id);
  if (index >= 0) {
    profiles.value[index] = updated;
  } else {
    profiles.value.push(updated);
  }
}

function updateProfileLocal(updated: WindsurfProfile) {
  const target = profiles.value.find(item => item.profile.id === updated.id);
  if (target) {
    target.profile = updated;
  }
}

async function loadProfiles() {
  loading.value = true;
  try {
    profiles.value = await profileApi.listProfiles();
    normalizeProfilePage();
  } catch (error) {
    ElMessage.error(`加载分身失败: ${error}`);
  } finally {
    loading.value = false;
  }
}

async function refreshProfileStatusSilently() {
  if (loading.value || actionLoading.value) return;
  try {
    profiles.value = await profileApi.listProfiles();
    normalizeProfilePage();
  } catch (error) {
    console.warn('刷新分身状态失败:', error);
  }
}

function openCreateDialog() {
  profileNameInput.value = '';
  createDialogVisible.value = true;
}

function openRenameDialog(profile: WindsurfProfile) {
  editingProfileId.value = profile.id;
  profileNameInput.value = profile.name;
  renameDialogVisible.value = true;
}

async function handleCreate() {
  const name = profileNameInput.value.trim();
  if (!name) {
    ElMessage.warning('请输入分身名称');
    return;
  }
  actionLoading.value = 'create';
  try {
    const created = await profileApi.createProfile(name);
    upsertProfileRuntime(created);
    currentProfilePage.value = profilePageCount.value;
    createDialogVisible.value = false;
    ElMessage.success('分身已创建');
  } catch (error) {
    ElMessage.error(`创建失败: ${error}`);
  } finally {
    actionLoading.value = '';
  }
}

async function handleRename() {
  const name = profileNameInput.value.trim();
  if (!editingProfileId.value || !name) return;
  actionLoading.value = 'rename';
  try {
    const updated = await profileApi.renameProfile(editingProfileId.value, name);
    upsertProfileRuntime(updated);
    renameDialogVisible.value = false;
    ElMessage.success('分身已重命名');
  } catch (error) {
    ElMessage.error(`重命名失败: ${error}`);
  } finally {
    actionLoading.value = '';
  }
}

async function handleDelete(profileId: string) {
  try {
    await ElMessageBox.confirm('删除后会移除该分身的独立数据目录，确认继续？', '删除分身', {
      type: 'warning',
      confirmButtonText: '删除',
      cancelButtonText: '取消',
    });
    actionLoading.value = `delete:${profileId}`;
    const result = await profileApi.deleteProfile(profileId);
    if (!result.success) {
      ElMessage.warning(result.message || '删除失败');
      return;
    }
    profiles.value = profiles.value.filter(item => item.profile.id !== profileId);
    normalizeProfilePage();
    ElMessage.success('分身已删除');
  } catch (error) {
    if (error !== 'cancel') {
      ElMessage.error(`删除失败: ${error}`);
    }
  } finally {
    actionLoading.value = '';
  }
}

async function handleLaunch(profileId: string) {
  actionLoading.value = `launch:${profileId}`;
  try {
    const result = await profileApi.launchProfile(profileId);
    ElMessage.success(result.alreadyRunning ? '分身已在运行' : '已启动 Windsurf 分身');
    await loadProfiles();
  } catch (error) {
    ElMessage.error(`启动失败: ${error}`);
  } finally {
    actionLoading.value = '';
  }
}

async function handleStop(profileId: string) {
  actionLoading.value = `stop:${profileId}`;
  try {
    const result = await profileApi.stopProfile(profileId);
    if (result.success) {
      ElMessage.success(result.alreadyStopped ? '分身已关闭' : '已关闭 Windsurf 分身');
      await loadProfiles();
    } else {
      ElMessage.error('关闭失败');
    }
  } catch (error) {
    ElMessage.error(`关闭失败: ${error}`);
  } finally {
    actionLoading.value = '';
  }
}

async function handleBindAccount(profileId: string, accountId: string) {
  try {
    const updated = await profileApi.bindAccountToProfile(profileId, accountId || null);
    updateProfileLocal(updated);
    ElMessage.success(accountId ? '绑定账号已更新' : '已取消绑定');
  } catch (error) {
    ElMessage.error(`绑定失败: ${error}`);
  }
}

async function updateAutoSwitch(profileId: string, enabled: boolean, group: string, threshold: number, checkInterval?: number) {
  try {
    const updated = await profileApi.updateProfileAutoSwitchConfig(profileId, enabled, group, threshold, checkInterval);
    updateProfileLocal(updated);
    ElMessage.success('自动换号配置已保存');
    return updated;
  } catch (error) {
    ElMessage.error(`保存失败: ${error}`);
    return null;
  }
}

async function handleAutoSwitchGroupChange(item: ProfileRuntimeInfo, group: string) {
  const updated = await updateAutoSwitch(item.profile.id, item.profile.autoSwitch.enabled, group, item.profile.autoSwitch.threshold, item.profile.autoSwitch.checkInterval);
  if (!updated?.boundAccountId) return;
  const accountInGroup = accountsStore.accounts.some(account => account.id === updated.boundAccountId && account.group === group);
  if (!accountInGroup) {
    await handleBindAccount(item.profile.id, '');
  }
}

async function updateMainAutoSwitchSettings(patch: Partial<Settings>) {
  try {
    await settingsStore.updateSettings({
      ...settingsStore.settings,
      ...patch,
    });
    ElMessage.success('主实例自动换号配置已保存');
  } catch (error) {
    ElMessage.error(`保存失败: ${error}`);
  }
}

async function handleMainAutoSwitchGroupChange(group: string) {
  const currentId = settingsStore.settings.autoSwitchCurrentAccountId;
  const patch: Partial<Settings> = { autoSwitchGroup: group };
  if (currentId && !accountsStore.accounts.some(account => account.id === currentId && account.group === group)) {
    patch.autoSwitchCurrentAccountId = null;
  }
  await updateMainAutoSwitchSettings(patch);
}

async function handleMainSwitch(accountId?: string | null) {
  if (!accountId) return;
  actionLoading.value = `switch:${MAIN_PROFILE_ID}`;
  try {
    const result = await apiService.switchAccount(accountId);
    if (result.success) {
      ElMessage.success(result.message || '主实例切号成功');
      await Promise.all([
        accountsStore.loadAccounts(),
        settingsStore.loadSettings(),
        loadProfiles(),
      ]);
    } else {
      ElMessage.error(result.error || '主实例切号失败');
    }
  } catch (error) {
    ElMessage.error(`主实例切号失败: ${error}`);
  } finally {
    actionLoading.value = '';
  }
}

async function handleSwitch(profileId: string, accountId?: string | null) {
  if (!accountId) return;
  actionLoading.value = `switch:${profileId}`;
  try {
    const result = await profileApi.switchAccountInProfile(profileId, accountId);
    if (result.success) {
      ElMessage.success(result.message || '分身切号成功');
      await loadProfiles();
    } else {
      ElMessage.error(result.error || '分身切号失败');
    }
  } catch (error) {
    ElMessage.error(`分身切号失败: ${error}`);
  } finally {
    actionLoading.value = '';
  }
}

async function handleMainCheckAutoSwitch() {
  actionLoading.value = `auto:${MAIN_PROFILE_ID}`;
  try {
    const result = await apiService.checkAutoSwitch();
    if (result.action === 'switched') {
      ElMessage.success(`已自动切换到 ${result.to_account}`);
      await Promise.all([
        accountsStore.loadAccounts(),
        settingsStore.loadSettings(),
        loadProfiles(),
      ]);
    } else if (result.action === 'error') {
      ElMessage.error(result.reason || '自动换号失败');
    } else {
      ElMessage.info(result.reason || '当前无需自动换号');
    }
  } catch (error) {
    ElMessage.error(`自动检测失败: ${error}`);
  } finally {
    actionLoading.value = '';
  }
}

async function handleCheckAutoSwitch(profileId: string) {
  actionLoading.value = `auto:${profileId}`;
  try {
    const result = await profileApi.checkProfileAutoSwitch(profileId);
    if (result.action === 'switched') {
      ElMessage.success(`已自动切换到 ${result.to_account}`);
      await loadProfiles();
    } else if (result.action === 'error') {
      ElMessage.error(result.reason || '自动换号失败');
    } else {
      ElMessage.info(result.reason || '当前无需自动换号');
    }
  } catch (error) {
    ElMessage.error(`自动检测失败: ${error}`);
  } finally {
    actionLoading.value = '';
  }
}

onMounted(async () => {
  await Promise.all([
    accountsStore.loadAccounts(),
    settingsStore.loadGroups(),
    loadProfiles(),
  ]);
  profileStatusTimer = setInterval(refreshProfileStatusSilently, 5000);
});

onUnmounted(() => {
  if (profileStatusTimer) {
    clearInterval(profileStatusTimer);
    profileStatusTimer = null;
  }
});
</script>

<style scoped>
.profile-manager-panel {
  height: 100%;
  min-height: 100%;
  padding: 28px;
  overflow-y: auto;
  background:
    radial-gradient(circle at top left, rgba(64, 158, 255, 0.18), transparent 34%),
    linear-gradient(135deg, rgba(8, 20, 36, 0.04), rgba(64, 158, 255, 0.05));
}

.profile-hero {
  display: flex;
  justify-content: space-between;
  gap: 24px;
  align-items: flex-start;
  padding: 28px;
  border: 1px solid rgba(64, 158, 255, 0.18);
  border-radius: 22px;
  background: linear-gradient(135deg, rgba(255, 255, 255, 0.96), rgba(244, 249, 255, 0.92));
  box-shadow: 0 22px 60px rgba(31, 79, 133, 0.12);
}

.eyebrow {
  color: #409eff;
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.16em;
  text-transform: uppercase;
}

.profile-hero h1 {
  margin: 8px 0;
  color: #182235;
  font-size: 30px;
  line-height: 1.1;
}

.profile-hero p {
  max-width: 680px;
  margin: 0;
  color: #667085;
  font-size: 14px;
}

.hero-actions {
  display: flex;
  gap: 10px;
  flex-wrap: wrap;
  justify-content: flex-end;
}

.profile-alert {
  margin: 18px 0;
  border-radius: 12px;
}

.profile-loading {
  display: flex;
  justify-content: center;
  padding: 72px;
}

.profile-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(420px, 1fr));
  gap: 18px;
}

.profile-list {
  display: flex;
  flex-direction: column;
  gap: 18px;
}

.profile-pagination {
  display: flex;
  justify-content: center;
  padding: 4px 0 10px;
}

.profile-card {
  border-radius: 18px;
  overflow: hidden;
}

.profile-card :deep(.el-card__header) {
  background: linear-gradient(135deg, rgba(248, 251, 255, 1), rgba(238, 247, 255, 0.92));
}

.main-profile {
  border-color: rgba(144, 147, 153, 0.36);
}

.profile-card-header {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  align-items: flex-start;
}

.profile-title-block {
  min-width: 0;
}

.profile-name-row {
  display: flex;
  gap: 8px;
  align-items: center;
}

.profile-name {
  color: #1f2937;
  font-size: 17px;
  font-weight: 700;
}

.profile-path {
  max-width: 320px;
  margin-top: 6px;
  overflow: hidden;
  color: #8a93a3;
  font-family: Consolas, monospace;
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.profile-status {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 10px;
  margin-bottom: 18px;
}

.status-item {
  min-width: 0;
  padding: 12px;
  border-radius: 12px;
  background: #f7f9fc;
}

.status-item .label {
  display: block;
  margin-bottom: 6px;
  color: #8a93a3;
  font-size: 12px;
}

.status-item strong {
  display: block;
  overflow: hidden;
  color: #1f2937;
  font-size: 13px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.profile-config {
  padding: 14px 14px 2px;
  border: 1px solid #eef2f7;
  border-radius: 14px;
  background: #fbfdff;
}

.profile-config :deep(.el-select),
.profile-config :deep(.el-input-number) {
  width: 100%;
}

.form-tip {
  margin-top: 5px;
  color: #909399;
  font-size: 12px;
  line-height: 1.5;
}

.profile-actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  margin-top: 16px;
}

@media (max-width: 900px) {
  .profile-manager-panel {
    padding: 16px;
  }

  .profile-hero {
    flex-direction: column;
  }

  .profile-grid {
    grid-template-columns: 1fr;
  }

  .profile-status {
    grid-template-columns: 1fr;
  }
}

:global(.dark) .profile-manager-panel {
  background:
    radial-gradient(circle at top left, rgba(64, 158, 255, 0.16), transparent 36%),
    linear-gradient(135deg, rgba(14, 20, 31, 1), rgba(11, 16, 24, 1));
}

:global(.dark) .profile-hero,
:global(.dark) .profile-card :deep(.el-card__header) {
  background: linear-gradient(135deg, rgba(29, 35, 45, 0.96), rgba(22, 29, 39, 0.92));
}

:global(.dark) .profile-hero h1,
:global(.dark) .profile-name,
:global(.dark) .status-item strong {
  color: #edf2f7;
}

:global(.dark) .profile-hero p,
:global(.dark) .profile-path,
:global(.dark) .status-item .label {
  color: #a8b3c4;
}

:global(.dark) .status-item,
:global(.dark) .profile-config {
  border-color: rgba(255, 255, 255, 0.08);
  background: rgba(255, 255, 255, 0.04);
}
</style>
