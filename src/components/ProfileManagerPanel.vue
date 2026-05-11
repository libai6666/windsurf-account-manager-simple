<template>
  <div class="profile-manager-panel">
    <div class="profile-hero">
      <div>
        <div class="eyebrow">Windsurf Profiles</div>
        <h1>分身管理</h1>
        <p>
          适合<strong>同时开发多个项目</strong>的多账号用户：每个分身 = 一个独立的 Windsurf 实例，账号、扩展、机器码完全隔离，互不影响。
          <span class="hero-tip">只开发一个项目用主实例就够了，无需创建分身。</span>
        </p>
      </div>
      <div class="hero-actions">
        <el-button :icon="Refresh" :loading="loading" @click="loadProfiles">刷新</el-button>
        <el-button type="primary" :icon="Plus" @click="openCreateDialog">新建分身</el-button>
      </div>
    </div>

    <el-alert
      type="warning"
      :closable="false"
      show-icon
      class="profile-alert profile-alert-tip"
    >
      <template #title>
        <div class="alert-title-row">
          <span class="alert-title-strong">📌 新建分身使用指南</span>
          <el-button text size="small" class="alert-toggle" @click="toggleTipExpanded">
            {{ tipExpanded ? '收起' : '展开完整步骤' }}
          </el-button>
        </div>
      </template>
      <template #default>
        <div v-if="!tipExpanded" class="alert-body-compact">
          推荐<strong>开启自动换号</strong>。新建分身后选好<strong>「换号分组」+「手动目标号」</strong>，点<strong>「切到目标号」</strong>，等 Windsurf 弹窗里点 <strong>Log in</strong>；macOS 如弹出浏览器，直接关闭浏览器并等待切号完成。
        </div>
        <div v-else class="alert-body">
          <div>每个分身使用独立 <code>--user-data-dir</code>，账号、机器码、扩展状态彼此隔离。建议<strong>开启「自动换号」</strong>，让管理器自动维护账号配额。</div>
          <div class="alert-info-line">
            ✨ 新建分身时会<strong>自动从主实例复制</strong> <code>settings.json</code> / <code>keybindings.json</code> / <code>snippets/</code>，Windows / macOS 通用，无需重新配置主题、快捷键、禁用更新等。
          </div>
          <div class="alert-info-line">
            ℹ️ 首次启动分身偶尔会弹 <code>connection to server is erroring</code> 提示，这是 Windsurf 自身扩展冷启动的已知现象，不影响登录和使用。
          </div>
          <div class="alert-highlight">
            <div class="alert-highlight-title">首次登录新分身的标准流程：</div>
            <ol class="step-list">
              <li><span class="step-tag">1</span><span>点击右上角 <strong>「新建分身」</strong>，输入名称后保存</span></li>
              <li><span class="step-tag">2</span><span>在分身卡片里选好 <strong>「换号分组」</strong> 和 <strong>「手动目标号」</strong>（要登录的账号）</span></li>
              <li><span class="step-tag">3</span><span>点击底部 <strong>「切到目标号」</strong>，管理器会自动启动分身窗口并把登录回调定向到该分身</span></li>
              <li><span class="step-tag">4</span><span>等待新 Windsurf 窗口弹出，在 Sign in 页面点击 <strong>「Log in」</strong> 按钮；macOS 首次可能会拉起浏览器，这是 Windsurf 自身登录按钮行为</span></li>
              <li><span class="step-tag">5</span><span>如果弹出浏览器，<strong>直接关闭浏览器，不要在浏览器里登录</strong>，回到分身窗口/管理器等待切号回调自动完成</span></li>
              <li><span class="step-tag">6</span><span>Windsurf 会自动用刚才选好的账号完成登录，<strong>无需手动输入邮箱/密码</strong></span></li>
            </ol>
            <div class="alert-tip-line">⚠️ 如果忘了第 2 步直接启动，会进入 Windsurf 默认登录页 — 这时关闭分身重做即可。</div>
          </div>
        </div>
      </template>
    </el-alert>

    <div class="auto-continue-panel">
      <div class="auto-continue-main">
        <div class="auto-continue-title-row">
          <span class="auto-continue-title">自动继续工作</span>
          <el-tag size="small" type="success">全局</el-tag>
        </div>
        <div class="auto-continue-desc">
          搭配自动换号使用：自动换号切到可用账号后，由 Windsurf 内部 Bridge 捕获页面中的模型异常、额度耗尽或试用用户全局限流等中断事件，并在当前 Cascade 输入框自动填入并提交“继续工作”；不再扫描系统窗口或截图。
        </div>
        <div class="auto-continue-hints">
          <span>内部文本捕获</span>
          <span>自动填入提交</span>
          <span>无需辅助功能权限</span>
        </div>
      </div>
      <div class="auto-continue-actions">
        <el-switch
          :model-value="autoContinueSwitchValue"
          active-text="开启"
          inactive-text="关闭"
          :loading="autoContinueLoading"
          :disabled="!autoContinueBridgePatched"
          @change="setAutoContinueEnabled(Boolean($event))"
        />
        <el-button :loading="autoContinueLoading" :icon="RefreshRight" @click="runAutoContinue(true)">
          检查Bridge
        </el-button>
        <div v-if="autoContinueLastMessage" class="auto-continue-status">
          {{ autoContinueLastMessage }}
        </div>
        <div v-if="autoContinuePatchChecked && !autoContinueBridgePatched" class="auto-continue-warning">
          需要先在设置中安装自动继续 Bridge 补丁，安装后才能开启
        </div>
      </div>
    </div>

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
          <div class="status-item status-item-email">
            <span class="label">实际登录账号</span>
            <el-tooltip
              v-if="item.currentInfo?.email"
              :content="item.currentInfo.email"
              placement="top"
              :show-after="300"
            >
              <strong>{{ item.currentInfo.email }}</strong>
            </el-tooltip>
            <strong v-else>未检测到</strong>
          </div>
          <div class="status-item status-item-plan">
            <span class="label">套餐</span>
            <strong>{{ item.currentInfo?.plan_name || '-' }}</strong>
          </div>
          <div class="status-item status-item-email">
            <span class="label">{{ item.profile.id === MAIN_PROFILE_ID ? '自动换号' : '手动目标号' }}</span>
            <strong v-if="item.profile.id === MAIN_PROFILE_ID">{{ settingsStore.settings.autoSwitchEnabled ? '已开启' : '未开启' }}</strong>
            <el-tooltip
              v-else-if="item.profile.boundAccountId"
              :content="boundAccountEmail(item.profile.boundAccountId)"
              placement="top"
              :show-after="300"
            >
              <strong>{{ boundAccountEmail(item.profile.boundAccountId) }}</strong>
            </el-tooltip>
            <strong v-else>未绑定</strong>
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
            <el-form-item label="换号分组">
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
            <el-form-item label="手动目标号">
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
                  :label="accountOptionLabel(account, MAIN_PROFILE_ID)"
                  :value="account.id"
                />
              </el-select>
              <div class="form-tip">仅作为手动切号目标；自动换号始终读取上方实际登录账号判断</div>
            </el-form-item>
            <el-form-item label="阈值">
              <div class="input-with-suffix">
                <el-input-number
                  :model-value="settingsStore.settings.autoSwitchThreshold"
                  :min="0"
                  :max="99"
                  :step="1"
                  @change="updateMainAutoSwitchSettings({ autoSwitchThreshold: Number($event ?? 5) })"
                />
                <span class="input-suffix">%</span>
              </div>
            </el-form-item>
            <el-form-item label="检测间隔">
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
                  :label="accountOptionLabel(account, item.profile.id)"
                  :value="account.id"
                />
              </el-select>
              <div class="form-tip">仅作为手动切号目标；自动换号会根据实际登录账号和分组配额判断</div>
            </el-form-item>
            <el-form-item label="阈值">
              <div class="input-with-suffix">
                <el-input-number
                  :model-value="item.profile.autoSwitch.threshold"
                  :min="0"
                  :max="100"
                  @change="updateAutoSwitch(item.profile.id, item.profile.autoSwitch.enabled, item.profile.autoSwitch.group, Number($event ?? 5), item.profile.autoSwitch.checkInterval)"
                />
                <span class="input-suffix">%</span>
              </div>
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
import { invoke } from '@tauri-apps/api/core';
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
let autoContinueTimer: ReturnType<typeof setInterval> | null = null;

const TIP_EXPANDED_KEY = 'profile-manager:tip-expanded';
const AUTO_CONTINUE_ENABLED_KEY = 'profile-manager:auto-continue-enabled';
const autoContinueEnabled = ref(Boolean(
  settingsStore.settings.autoContinueBridgeEnabled ?? (localStorage.getItem(AUTO_CONTINUE_ENABLED_KEY) === '1'),
));
const autoContinueBridgePatched = ref(false);
const autoContinuePatchChecked = ref(false);
const autoContinueLoading = ref(false);
const autoContinueLastMessage = ref('');
const autoContinueSwitchValue = computed(() => autoContinueBridgePatched.value && autoContinueEnabled.value);
const tipExpanded = ref<boolean>(
  // 默认展开（首次访问看完整指南）；用户主动收起后持久化
  localStorage.getItem(TIP_EXPANDED_KEY) !== '0',
);
function toggleTipExpanded() {
  tipExpanded.value = !tipExpanded.value;
  localStorage.setItem(TIP_EXPANDED_KEY, tipExpanded.value ? '1' : '0');
}

const accountEmailMap = computed(() => {
  const map = new Map<string, string>();
  for (const account of accountsStore.accounts) {
    map.set(account.id, account.email);
  }
  return map;
});

const accountIdByEmail = computed(() => {
  const map = new Map<string, string>();
  for (const account of accountsStore.accounts) {
    map.set(account.email.toLowerCase(), account.id);
  }
  return map;
});

/// account.id → { profileId, profileName }
/// 包含：每个 profile 实际登录账号、每个分身的 boundAccountId、主实例的 autoSwitchCurrentAccountId
const accountUsageMap = computed(() => {
  const map = new Map<string, { profileId: string; profileName: string }>();
  const setIfAbsent = (accountId: string, profileId: string, profileName: string) => {
    if (!accountId) return;
    if (!map.has(accountId)) map.set(accountId, { profileId, profileName });
  };

  for (const item of profiles.value) {
    const profileLabel = item.profile.id === MAIN_PROFILE_ID ? '主实例' : item.profile.name;
    if (item.currentInfo?.email) {
      const accId = accountIdByEmail.value.get(item.currentInfo.email.toLowerCase());
      if (accId) setIfAbsent(accId, item.profile.id, profileLabel);
    }
    if (item.profile.id !== MAIN_PROFILE_ID && item.profile.boundAccountId) {
      setIfAbsent(item.profile.boundAccountId, item.profile.id, profileLabel);
    }
  }

  if (settingsStore.settings.autoSwitchCurrentAccountId) {
    setIfAbsent(settingsStore.settings.autoSwitchCurrentAccountId, MAIN_PROFILE_ID, '主实例');
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

function accountOptionLabel(account: Account, currentProfileId?: string) {
  const daily = account.daily_quota_remaining;
  const weekly = account.weekly_quota_remaining;
  const base = (daily === undefined && weekly === undefined)
    ? account.email
    : `${account.email} (日${daily ?? '?'}%/周${weekly ?? '?'}%)`;
  const usage = accountUsageMap.value.get(account.id);
  if (usage && usage.profileId !== currentProfileId) {
    return `${base} 【已被${usage.profileName}使用】`;
  }
  return base;
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

async function runAutoContinue(showMessage = false) {
  if (autoContinueLoading.value) return;
  autoContinueLoading.value = true;
  try {
    const bridgePatched = await refreshAutoContinuePatchStatus(false);
    if (!bridgePatched) {
      await forceAutoContinueDisabled('Bridge补丁未安装，自动继续已关闭；请先在设置中安装补丁后再开启');
      if (showMessage) {
        ElMessage.warning(autoContinueLastMessage.value);
      }
      return;
    }
    const result = await profileApi.getAutoContinueBridgeStatus();
    autoContinueLastMessage.value = result.message || 'Bridge状态已刷新';
    await syncAutoContinueEnabled(Boolean(result.config?.enabled));
    if (showMessage) {
      if (result.config?.enabled) {
        ElMessage.success(autoContinueLastMessage.value);
      } else {
        ElMessage.info(autoContinueLastMessage.value);
      }
    }
  } catch (error) {
    autoContinueLastMessage.value = `自动继续失败: ${error}`;
    if (showMessage) {
      ElMessage.error(autoContinueLastMessage.value);
    }
  } finally {
    autoContinueLoading.value = false;
  }
}

async function refreshAutoContinuePatchStatus(showMessage = false) {
  const windsurfPath = settingsStore.settings.windsurfPath;
  if (!windsurfPath) {
    autoContinueBridgePatched.value = false;
    autoContinuePatchChecked.value = true;
    autoContinueLastMessage.value = '请先在设置中检测或选择 Windsurf 路径，并安装自动继续 Bridge 补丁';
    if (showMessage) {
      ElMessage.warning(autoContinueLastMessage.value);
    }
    return false;
  }

  try {
    const status = await invoke<any>('check_patch_status', { windsurfPath });
    autoContinueBridgePatched.value = Boolean(status.auto_continue_bridge);
    autoContinuePatchChecked.value = true;
    if (!autoContinueBridgePatched.value) {
      autoContinueLastMessage.value = 'Bridge补丁未安装，自动继续已关闭；请先在设置中安装补丁后再开启';
      if (showMessage) {
        ElMessage.warning(autoContinueLastMessage.value);
      }
      return false;
    }
    return true;
  } catch (error) {
    autoContinueBridgePatched.value = false;
    autoContinuePatchChecked.value = true;
    autoContinueLastMessage.value = `检查Bridge补丁失败: ${error}`;
    if (showMessage) {
      ElMessage.error(autoContinueLastMessage.value);
    }
    return false;
  }
}

async function syncAutoContinueEnabled(enabled: boolean) {
  autoContinueEnabled.value = enabled;
  localStorage.setItem(AUTO_CONTINUE_ENABLED_KEY, enabled ? '1' : '0');
  if (settingsStore.settings.autoContinueBridgeEnabled !== enabled) {
    await settingsStore.updateSettings({
      ...settingsStore.settings,
      autoContinueBridgeEnabled: enabled,
    });
  }
}

async function forceAutoContinueDisabled(message?: string) {
  stopAutoContinueTimer();
  try {
    await profileApi.setAutoContinueBridgeConfig(false);
  } catch (error) {
    console.warn('关闭自动继续Bridge失败:', error);
  }
  await syncAutoContinueEnabled(false);
  if (message) {
    autoContinueLastMessage.value = message;
  }
}

function stopAutoContinueTimer() {
  if (autoContinueTimer) {
    clearInterval(autoContinueTimer);
    autoContinueTimer = null;
  }
}

function startAutoContinueTimer() {
  stopAutoContinueTimer();
  autoContinueTimer = setInterval(() => {
    if (autoContinueEnabled.value) {
      void runAutoContinue(false);
    }
  }, 30000);
}

async function setAutoContinueEnabled(enabled: boolean) {
  if (enabled) {
    autoContinueLoading.value = true;
    try {
      const bridgePatched = await refreshAutoContinuePatchStatus(false);
      if (!bridgePatched) {
        await forceAutoContinueDisabled('Bridge补丁未安装，无法开启自动继续；请先在设置中安装补丁');
        ElMessage.warning(autoContinueLastMessage.value);
        return;
      }
    } finally {
      autoContinueLoading.value = false;
    }
  }
  autoContinueEnabled.value = enabled;
  localStorage.setItem(AUTO_CONTINUE_ENABLED_KEY, enabled ? '1' : '0');
  autoContinueLoading.value = true;
  try {
    const result = await profileApi.setAutoContinueBridgeConfig(enabled);
    autoContinueLastMessage.value = result.message || (enabled ? '自动继续Bridge已开启' : '自动继续Bridge已关闭');
    await settingsStore.updateSettings({
      ...settingsStore.settings,
      autoContinueBridgeEnabled: enabled,
    });
  } catch (error) {
    autoContinueEnabled.value = !enabled;
    localStorage.setItem(AUTO_CONTINUE_ENABLED_KEY, !enabled ? '1' : '0');
    autoContinueLastMessage.value = `自动继续Bridge配置失败: ${error}`;
    ElMessage.error(autoContinueLastMessage.value);
  } finally {
    autoContinueLoading.value = false;
  }
  if (enabled) {
    startAutoContinueTimer();
  } else {
    stopAutoContinueTimer();
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
  const target = profiles.value.find(item => item.profile.id === profileId);
  const targetName = target?.profile.name || '该分身';
  try {
    await ElMessageBox.confirm(
      `确定要关闭 ${targetName} 吗？正在编辑的内容如未保存可能丢失。`,
      '确认关闭',
      {
        confirmButtonText: '关闭',
        cancelButtonText: '取消',
        type: 'warning',
      },
    );
  } catch {
    return;
  }
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
  await runAutoContinue(false);
  if (autoContinueEnabled.value) {
    startAutoContinueTimer();
  }
});

onUnmounted(() => {
  if (profileStatusTimer) {
    clearInterval(profileStatusTimer);
    profileStatusTimer = null;
  }
  stopAutoContinueTimer();
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
  max-width: 720px;
  margin: 0;
  color: #667085;
  font-size: 13.5px;
  line-height: 1.65;
}

.profile-hero p strong {
  color: #1d4ed8;
  font-weight: 600;
}

.hero-tip {
  display: inline-block;
  margin-left: 4px;
  padding: 1px 8px;
  border-radius: 999px;
  background: rgba(64, 158, 255, 0.12);
  color: #1d4ed8;
  font-size: 12.5px;
  white-space: nowrap;
}

.hero-actions {
  display: flex;
  gap: 10px;
  flex-wrap: wrap;
  justify-content: flex-end;
}

.profile-alert {
  margin: 14px 0;
  border-radius: 10px;
}

.profile-alert-tip {
  padding: 8px 12px !important;
  border: 1px solid #fbbf24;
  background: linear-gradient(135deg, #fffbeb 0%, #fef3c7 100%) !important;
}

.profile-alert-tip :deep(.el-alert__content) {
  padding: 0;
}

.profile-alert-tip :deep(.el-alert__title) {
  font-size: 13px;
}

.auto-continue-panel {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  margin: 14px 0;
  padding: 16px 18px;
  border: 1px solid rgba(103, 194, 58, 0.2);
  border-radius: 16px;
  background: linear-gradient(135deg, rgba(240, 249, 235, 0.94), rgba(255, 255, 255, 0.9));
  box-shadow: 0 12px 32px rgba(46, 125, 50, 0.08);
}

.auto-continue-main {
  flex: 1 1 auto;
  min-width: 0;
  max-width: 560px;
}

.auto-continue-title-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 6px;
}

.auto-continue-title {
  color: #1f4d2b;
  font-size: 15px;
  font-weight: 700;
}

.auto-continue-desc {
  max-width: 520px;
  color: #4b5563;
  font-size: 12.5px;
  line-height: 1.6;
}

.auto-continue-desc strong {
  color: #15803d;
  font-weight: 700;
}

.auto-continue-hints {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 8px;
}

.auto-continue-hints span {
  padding: 2px 8px;
  border-radius: 999px;
  background: rgba(103, 194, 58, 0.12);
  color: #2f6b3a;
  font-size: 11.5px;
}

.auto-continue-actions {
  display: flex;
  flex: 0 0 430px;
  align-items: center;
  justify-content: flex-end;
  flex-wrap: wrap;
  gap: 8px;
  min-width: 360px;
}

.auto-continue-status {
  max-width: 150px;
  overflow: hidden;
  color: #4b5563;
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.auto-continue-warning {
  flex-basis: 100%;
  color: #b45309;
  font-size: 11.5px;
  line-height: 1.4;
  text-align: right;
}

.alert-title-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.alert-title-strong {
  color: #b45309;
  font-weight: 700;
  letter-spacing: 0.2px;
}

.alert-toggle {
  height: 22px !important;
  padding: 0 6px !important;
  color: #b45309 !important;
  font-size: 12px !important;
}

.alert-toggle:hover {
  color: #92400e !important;
  background: rgba(251, 191, 36, 0.12) !important;
}

.alert-body-compact {
  margin-top: 4px;
  color: #374151;
  font-size: 12.5px;
  line-height: 1.55;
}

.alert-body-compact strong {
  color: #b45309;
}

.alert-body {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-top: 4px;
  color: #374151;
  font-size: 12.5px;
  line-height: 1.55;
}

.alert-body code {
  padding: 0 4px;
  border-radius: 3px;
  background: rgba(180, 83, 9, 0.1);
  color: #b45309;
  font-family: Consolas, monospace;
  font-size: 11.5px;
}

.alert-info-line {
  padding: 5px 8px;
  border-left: 2px solid rgba(180, 83, 9, 0.4);
  background: rgba(255, 255, 255, 0.45);
  border-radius: 3px;
  color: #374151;
  font-size: 12px;
  line-height: 1.6;
}

.alert-info-line strong {
  color: #b45309;
  font-weight: 600;
}

.alert-info-line code {
  padding: 0 4px;
  margin: 0 1px;
  border-radius: 3px;
  background: rgba(180, 83, 9, 0.1);
  color: #b45309;
  font-family: 'JetBrains Mono', 'Fira Code', Consolas, monospace;
  font-size: 11px;
}

.alert-highlight {
  padding: 6px 10px;
  border-left: 3px solid #f59e0b;
  border-radius: 4px;
  background: rgba(255, 255, 255, 0.65);
  color: #1f2937;
}

.alert-highlight-title {
  margin-bottom: 4px;
  color: #b45309;
  font-size: 12.5px;
  font-weight: 700;
}

.alert-highlight strong {
  color: #b45309;
}

.step-list {
  margin: 0;
  padding: 0;
  list-style: none;
}

.step-list li {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  padding: 1px 0;
  line-height: 1.5;
}

.step-tag {
  display: inline-flex;
  flex-shrink: 0;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  margin-top: 2px;
  border-radius: 50%;
  background: #f59e0b;
  color: #fff;
  font-size: 11px;
  font-weight: 700;
  line-height: 1;
}

.alert-tip-line {
  margin-top: 6px;
  padding-top: 5px;
  border-top: 1px dashed rgba(180, 83, 9, 0.25);
  color: #92400e;
  font-size: 11.5px;
}

:global(.dark) .profile-alert-tip {
  border-color: rgba(251, 191, 36, 0.3);
  background: linear-gradient(135deg, rgba(120, 53, 15, 0.2) 0%, rgba(146, 64, 14, 0.2) 100%) !important;
}

:global(.dark) .auto-continue-panel {
  border-color: rgba(74, 222, 128, 0.2);
  background: linear-gradient(135deg, rgba(20, 83, 45, 0.18), rgba(15, 23, 42, 0.82));
}

:global(.dark) .auto-continue-title,
:global(.dark) .auto-continue-desc strong {
  color: #86efac;
}

:global(.dark) .auto-continue-desc,
:global(.dark) .auto-continue-status {
  color: #d1d5db;
}

:global(.dark) .auto-continue-hints span {
  background: rgba(74, 222, 128, 0.12);
  color: #bbf7d0;
}

:global(.dark) .alert-title-strong,
:global(.dark) .alert-highlight strong,
:global(.dark) .alert-highlight-title {
  color: #fbbf24;
}

:global(.dark) .step-tag {
  background: #fbbf24;
  color: #1f2937;
}

:global(.dark) .alert-tip-line {
  border-top-color: rgba(251, 191, 36, 0.3);
  color: #fcd34d;
}

:global(.dark) .alert-body,
:global(.dark) .alert-body-compact {
  color: #e5e7eb;
}

:global(.dark) .alert-info-line {
  background: rgba(255, 255, 255, 0.04);
  border-left-color: rgba(251, 191, 36, 0.45);
  color: #e5e7eb;
}

:global(.dark) .alert-info-line strong {
  color: #fbbf24;
}

:global(.dark) .alert-info-line code {
  background: rgba(251, 191, 36, 0.15);
  color: #fbbf24;
}

:global(.dark) .alert-body-compact strong {
  color: #fbbf24;
}

:global(.dark) .alert-toggle {
  color: #fbbf24 !important;
}

:global(.dark) .alert-toggle:hover {
  background: rgba(251, 191, 36, 0.15) !important;
}

:global(.dark) .alert-body code {
  background: rgba(251, 191, 36, 0.15);
  color: #fbbf24;
}

:global(.dark) .alert-highlight {
  background: rgba(255, 255, 255, 0.05);
  color: #e5e7eb;
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
  grid-template-columns: minmax(0, 2.2fr) minmax(0, 1fr) minmax(0, 2.2fr);
  gap: 8px;
  margin-bottom: 18px;
}

.status-item {
  min-width: 0;
  padding: 10px 12px;
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
  min-width: 0;
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

.input-with-suffix {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
}

.input-with-suffix :deep(.el-input-number) {
  flex: 1;
}

.input-suffix {
  flex-shrink: 0;
  color: #6b7280;
  font-size: 14px;
  font-weight: 500;
}

:global(.dark) .input-suffix {
  color: #cbd5e1;
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

  .auto-continue-panel {
    flex-direction: column;
    align-items: stretch;
  }

  .auto-continue-actions {
    justify-content: flex-start;
    min-width: 0;
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

:global(.dark) .profile-hero p strong,
:global(.dark) .hero-tip {
  color: #93c5fd;
}

:global(.dark) .hero-tip {
  background: rgba(147, 197, 253, 0.15);
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
