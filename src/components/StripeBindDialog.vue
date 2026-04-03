<template>
  <el-dialog
    v-model="visible"
    title="协议绑卡"
    width="900px"
    :close-on-click-modal="false"
    destroy-on-close
    @close="handleClose"
  >
    <el-tabs v-model="activeTab" type="border-card">
      <!-- ═══ Tab 1: 配置 ═══ -->
      <el-tab-pane label="配置" name="config">
        <el-form label-width="100px" size="default">
          <!-- 账号选择 -->
          <el-form-item label="选择分组">
            <div style="display: flex; align-items: center; flex-wrap: wrap; gap: 6px; width: 100%">
              <el-select v-model="selectedGroup" placeholder="选择分组" style="width: 180px" @change="handleGroupChange">
                <el-option label="全部账号" value="__all__" />
                <el-option v-for="g in groups" :key="g" :label="g" :value="g" />
              </el-select>
              <el-button type="primary" plain size="small" @click="selectAllInGroup">全选</el-button>
              <el-button type="success" plain size="small" @click="selectFreeOnly">只选Free</el-button>
              <el-button size="small" @click="clearAccountSelection">清空</el-button>
              <el-button :icon="RefreshIcon" size="small" circle @click="refreshAccounts" :loading="isRefreshing" title="刷新已选账号状态" />
              <span style="color: #909399; font-size: 12px">
                已选 {{ selectedAccountIds.length }} / {{ filteredGroupAccounts.length }} 个账号
              </span>
            </div>
          </el-form-item>

          <el-form-item label="账号列表">
            <div class="account-select-box">
              <el-checkbox-group v-model="selectedAccountIds">
                <el-checkbox
                  v-for="acc in paginatedAccounts"
                  :key="acc.id"
                  :label="acc.id"
                  :value="acc.id"
                  class="account-checkbox"
                >
                  <span class="acc-email">{{ acc.email }}</span>
                  <el-tag v-if="acc.plan_name" size="small" :type="acc.plan_name === 'Free' ? 'info' : 'success'" style="margin-left: 4px">
                    {{ acc.plan_name }}
                  </el-tag>
                </el-checkbox>
              </el-checkbox-group>
              <el-empty v-if="filteredGroupAccounts.length === 0" description="该分组没有账号" :image-size="40" />
            </div>
            <!-- 分页 -->
            <div v-if="filteredGroupAccounts.length > pageSize" class="account-pagination">
              <el-pagination
                v-model:current-page="currentPage"
                :page-size="pageSize"
                :page-sizes="[10, 20, 50, 100, 500, 1000]"
                :total="filteredGroupAccounts.length"
                layout="sizes, prev, pager, next, jumper"
                size="small"
                @size-change="handlePageSizeChange"
              />
            </div>
          </el-form-item>

          <el-divider content-position="left">卡片管理 (可添加多张，轮询分配给账号)</el-divider>

          <!-- 添加/编辑卡片表单 -->
          <el-row :gutter="8" style="margin-bottom: 8px">
            <el-col :span="14">
              <el-input v-model="newCardRaw" placeholder="卡号|MM/YY|CVC  例如: 5253636962627026|08/30|571" size="small" @keyup.enter="editingCardIdx >= 0 ? saveEditCard() : addCard()" />
            </el-col>
            <el-col :span="6">
              <el-input v-model="newCardLabel" placeholder="备注(可选)" size="small" />
            </el-col>
            <el-col :span="4" v-if="editingCardIdx >= 0">
              <el-button type="success" size="small" :disabled="!canAddCard" @click="saveEditCard" style="width: 100%">保存修改</el-button>
            </el-col>
            <el-col :span="4" v-else>
              <el-button type="primary" size="small" :disabled="!canAddCard" @click="addCard" style="width: 100%">添加卡片</el-button>
            </el-col>
          </el-row>
          <div v-if="editingCardIdx >= 0" style="margin-bottom: 8px; font-size: 12px; color: #e6a23c">
            正在编辑第 {{ editingCardIdx + 1 }} 张卡。
            <el-button type="info" text size="small" @click="cancelEditCard">取消编辑</el-button>
          </div>

          <!-- 卡片列表 -->
          <div class="card-list-box">
            <el-checkbox-group v-model="selectedCardIndices">
              <div v-for="(card, idx) in savedCards" :key="idx" class="card-item">
                <el-checkbox :label="idx" :value="idx">
                  <span class="card-number">****{{ card.number.slice(-4) }}</span>
                  <span class="card-meta">{{ card.exp_month }}/{{ card.exp_year }} | CVC: {{ card.cvc }}</span>
                  <el-tag v-if="card.label" size="small" type="info" style="margin-left: 4px">{{ card.label }}</el-tag>
                </el-checkbox>
                <span style="margin-left: auto; display: flex; gap: 4px">
                  <el-button type="primary" text size="small" @click="startEditCard(idx)">编辑</el-button>
                  <el-button type="danger" text size="small" @click="removeCard(idx)">删除</el-button>
                </span>
              </div>
            </el-checkbox-group>
            <el-empty v-if="savedCards.length === 0" description="请添加卡片" :image-size="32" />
          </div>
          <div v-if="savedCards.length > 1 && selectedCardIndices.length > 1" style="margin-top: 4px; font-size: 12px; color: #909399">
            已选 {{ selectedCardIndices.length }} 张卡，将按顺序轮询分配给账号。例如: 账号1→卡1, 账号2→卡2, 账号3→卡1 ...
          </div>

          <el-divider content-position="left">账单地址 & 姓名</el-divider>
          <el-row style="margin-bottom: 8px">
            <el-checkbox v-model="useCustomAddress">手动填写地址</el-checkbox>
            <span style="margin-left: 8px; font-size: 12px; color: #909399">不勾选则每个账号自动生成独立的随机美国地址和姓名</span>
          </el-row>
          <template v-if="useCustomAddress">
            <el-row :gutter="12">
              <el-col :span="8">
                <el-form-item label="姓名">
                  <el-input v-model="customAddress.name" placeholder="John Smith" />
                </el-form-item>
              </el-col>
              <el-col :span="16">
                <el-form-item label="街道">
                  <el-input v-model="customAddress.line1" placeholder="123 Main St" />
                </el-form-item>
              </el-col>
            </el-row>
            <el-row :gutter="12">
              <el-col :span="8">
                <el-form-item label="城市">
                  <el-input v-model="customAddress.city" placeholder="New York" />
                </el-form-item>
              </el-col>
              <el-col :span="4">
                <el-form-item label="州">
                  <el-input v-model="customAddress.state" placeholder="NY" maxlength="2" />
                </el-form-item>
              </el-col>
              <el-col :span="4">
                <el-form-item label="邮编">
                  <el-input v-model="customAddress.postal_code" placeholder="10001" />
                </el-form-item>
              </el-col>
              <el-col :span="4">
                <el-form-item label="国家">
                  <el-input v-model="customAddress.country" placeholder="US" maxlength="2" />
                </el-form-item>
              </el-col>
            </el-row>
          </template>

          <el-divider content-position="left">打码平台 & 网络代理</el-divider>

          <el-row :gutter="12">
            <el-col :span="16">
              <el-form-item label="API Key">
                <el-input v-model="captchaConfig.api_key" placeholder="YesCaptcha API Key" show-password />
              </el-form-item>
            </el-col>
            <el-col :span="8">
              <el-form-item label="API URL">
                <el-input v-model="captchaConfig.api_url" placeholder="https://api.yescaptcha.com" />
              </el-form-item>
            </el-col>
          </el-row>

          <el-row :gutter="12">
            <el-col :span="8">
              <el-form-item label="代理Host">
                <el-input v-model="proxyConfig.host" placeholder="HTTP/SOCKS代理地址(可选)" />
              </el-form-item>
            </el-col>
            <el-col :span="4">
              <el-form-item label="端口">
                <el-input-number v-model="proxyConfig.port" :min="1" :max="65535" :controls="false" style="width: 100%" />
              </el-form-item>
            </el-col>
            <el-col :span="6">
              <el-form-item label="用户名">
                <el-input v-model="proxyConfig.user" placeholder="代理认证(可选)" />
              </el-form-item>
            </el-col>
            <el-col :span="6">
              <el-form-item label="密码">
                <el-input v-model="proxyConfig.pass" placeholder="代理认证(可选)" show-password />
              </el-form-item>
            </el-col>
          </el-row>

          <el-row :gutter="12">
            <el-col :span="8">
              <el-form-item label="套餐">
                <el-select v-model="teamsTier" style="width: 100%">
                  <el-option label="Pro" :value="2" />
                  <el-option label="Teams" :value="1" />
                </el-select>
              </el-form-item>
            </el-col>
            <el-col :span="8">
              <el-form-item label="付费周期">
                <el-select v-model="paymentPeriod" style="width: 100%">
                  <el-option label="月付" :value="1" />
                  <el-option label="年付" :value="2" />
                </el-select>
              </el-form-item>
            </el-col>
            <el-col :span="8">
              <el-form-item label="并发数">
                <el-input-number v-model="concurrency" :min="1" :max="5" style="width: 100%" />
              </el-form-item>
            </el-col>
          </el-row>
        </el-form>

        <div style="text-align: center; margin-top: 10px">
          <el-button
            type="primary"
            size="large"
            :loading="isRunning"
            :disabled="!canStart"
            @click="startBind"
          >
            开始批量绑卡 ({{ selectedAccountIds.length }} 个账号, {{ selectedCardIndices.length }} 张卡)
          </el-button>
        </div>
      </el-tab-pane>

      <!-- ═══ Tab 2: 执行 ═══ -->
      <el-tab-pane label="执行" name="execution">
        <div v-if="taskList.length === 0" style="text-align: center; padding: 40px">
          <el-empty description="暂无执行任务，请先在配置页启动" />
        </div>
        <div v-else>
          <div style="margin-bottom: 12px; display: flex; justify-content: space-between; align-items: center">
            <span style="font-size: 14px; color: #606266">
              总计: {{ taskList.length }} | 
              <span style="color: #67c23a">成功: {{ taskStats.success }}</span> | 
              <span style="color: #f56c6c">失败: {{ taskStats.failed }}</span> | 
              <span style="color: #409eff">进行中: {{ taskStats.running }}</span> | 
              <span style="color: #909399">等待: {{ taskStats.pending }}</span>
            </span>
            <el-button v-if="isRunning" type="danger" size="small" @click="cancelBind">取消</el-button>
          </div>

          <el-progress
            :percentage="overallProgress"
            :stroke-width="16"
            :format="() => `${taskStats.success + taskStats.failed} / ${taskList.length}`"
            style="margin-bottom: 16px"
          />

          <div class="task-list-container">
            <div
              v-for="task in taskList"
              :key="task.account_id"
              class="task-item"
              :class="'task-' + task.status"
            >
              <div class="task-email">{{ task.email }}</div>
              <div class="task-step">
                <el-tag
                  :type="taskTagType(task.status)"
                  size="small"
                  effect="dark"
                >
                  {{ task.status === 'running' ? `步骤 ${task.step}/6: ${task.step_name}` : taskStatusText(task.status) }}
                </el-tag>
              </div>
              <div v-if="task.error" class="task-error">{{ task.error }}</div>
            </div>
          </div>
        </div>
      </el-tab-pane>

      <!-- ═══ Tab 3: 日志 ═══ -->
      <el-tab-pane label="日志" name="logs">
        <div style="margin-bottom: 8px; display: flex; justify-content: space-between; align-items: center">
          <el-input
            v-model="logFilter"
            placeholder="搜索日志..."
            clearable
            size="small"
            style="width: 300px"
            :prefix-icon="SearchIcon"
          />
          <div>
            <el-button size="small" @click="clearLogs">清空</el-button>
            <el-button size="small" type="primary" @click="exportLogs">导出</el-button>
            <el-checkbox v-model="autoScroll" size="small" style="margin-left: 12px">自动滚动</el-checkbox>
          </div>
        </div>
        <div ref="logContainerRef" class="log-container">
          <div
            v-for="(log, idx) in filteredLogs"
            :key="idx"
            class="log-line"
            :class="'log-' + log.level"
          >
            <span class="log-time">{{ log.time }}</span>
            <span class="log-level">[{{ log.level.toUpperCase() }}]</span>
            <span class="log-msg">{{ log.message }}</span>
          </div>
          <div v-if="filteredLogs.length === 0" style="text-align: center; color: #909399; padding: 40px">
            暂无日志
          </div>
        </div>
      </el-tab-pane>
    </el-tabs>
  </el-dialog>

  <!-- 批量 Turnstile 人机验证弹窗 -->
  <ConcurrentTurnstileDialog
    :visible="showTurnstile"
    :accounts="turnstileAccounts"
    :initial-concurrency="4"
    @update:visible="showTurnstile = $event"
    @all-completed="onTurnstileCompleted"
    @close="onTurnstileClosed"
  />
</template>

<script setup lang="ts">
import { ref, computed, nextTick, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { ElMessage } from 'element-plus';
import { Search as SearchIcon, Refresh as RefreshIcon } from '@element-plus/icons-vue';
import { useAccountsStore, useSettingsStore } from '@/store';
import ConcurrentTurnstileDialog from '@/components/ConcurrentTurnstileDialog.vue';

const props = defineProps<{ modelValue: boolean }>();
const emit = defineEmits(['update:modelValue']);

const visible = computed({
  get: () => props.modelValue,
  set: (val: boolean) => emit('update:modelValue', val),
});

const accountsStore = useAccountsStore();
const settingsStore = useSettingsStore();

// ─── 配置 Tab ────────────────────────────────────
const activeTab = ref('config');
const selectedGroup = ref('__all__');
const selectedAccountIds = ref<string[]>([]);
const currentPage = ref(1);
const pageSize = ref(20);
const isRefreshing = ref(false);

// 多卡管理
interface SavedCard {
  number: string;
  cvc: string;
  exp_year: string;
  exp_month: string;
  label: string;
}

const CARDS_STORAGE_KEY = 'stripe_bind_saved_cards';
const CONFIG_STORAGE_KEY = 'stripe_bind_config';

const savedCards = ref<SavedCard[]>([]);
const selectedCardIndices = ref<number[]>([]);
const newCardRaw = ref('');
const newCardLabel = ref('');
const editingCardIdx = ref(-1);

function parseCardRaw(raw: string): SavedCard | null {
  const parts = raw.trim().split('|');
  if (parts.length < 3) return null;
  const number = parts[0].trim();
  const expParts = parts[1].trim().split('/');
  if (expParts.length < 2) return null;
  const exp_month = expParts[0].trim();
  let exp_year = expParts[1].trim();
  if (exp_year.length === 2) exp_year = '20' + exp_year;
  const cvc = parts[2].trim();
  if (number.length < 13 || cvc.length < 3 || exp_month.length < 1 || exp_year.length !== 4) return null;
  return { number, cvc, exp_year, exp_month, label: '' };
}

const canAddCard = computed(() => {
  return parseCardRaw(newCardRaw.value) !== null;
});

function loadSavedCards() {
  try {
    const raw = localStorage.getItem(CARDS_STORAGE_KEY);
    if (raw) savedCards.value = JSON.parse(raw);
  } catch { /* ignore */ }
}

function persistCards() {
  localStorage.setItem(CARDS_STORAGE_KEY, JSON.stringify(savedCards.value));
}

function addCard() {
  if (!canAddCard.value) return;
  const card = parseCardRaw(newCardRaw.value)!;
  card.label = newCardLabel.value.trim();
  savedCards.value.push(card);
  selectedCardIndices.value.push(savedCards.value.length - 1);
  newCardRaw.value = '';
  newCardLabel.value = '';
  persistCards();
}

function removeCard(idx: number) {
  savedCards.value.splice(idx, 1);
  selectedCardIndices.value = selectedCardIndices.value
    .filter(i => i !== idx)
    .map(i => i > idx ? i - 1 : i);
  if (editingCardIdx.value === idx) cancelEditCard();
  else if (editingCardIdx.value > idx) editingCardIdx.value--;
  persistCards();
}

function startEditCard(idx: number) {
  const card = savedCards.value[idx];
  newCardRaw.value = `${card.number}|${card.exp_month}/${card.exp_year}|${card.cvc}`;
  newCardLabel.value = card.label;
  editingCardIdx.value = idx;
}

function saveEditCard() {
  if (!canAddCard.value || editingCardIdx.value < 0) return;
  const card = parseCardRaw(newCardRaw.value)!;
  card.label = newCardLabel.value.trim();
  savedCards.value[editingCardIdx.value] = card;
  cancelEditCard();
  persistCards();
}

function cancelEditCard() {
  editingCardIdx.value = -1;
  newCardRaw.value = '';
  newCardLabel.value = '';
}

function loadConfig() {
  try {
    const raw = localStorage.getItem(CONFIG_STORAGE_KEY);
    if (raw) {
      const cfg = JSON.parse(raw);
      if (cfg.captcha) captchaConfig.value = cfg.captcha;
      if (cfg.proxy) proxyConfig.value = cfg.proxy;
      if (cfg.teamsTier != null) teamsTier.value = cfg.teamsTier;
      if (cfg.paymentPeriod != null) paymentPeriod.value = cfg.paymentPeriod;
      if (cfg.concurrency != null) concurrency.value = cfg.concurrency;
    }
  } catch { /* ignore */ }
}

function persistConfig() {
  localStorage.setItem(CONFIG_STORAGE_KEY, JSON.stringify({
    captcha: captchaConfig.value,
    proxy: proxyConfig.value,
    teamsTier: teamsTier.value,
    paymentPeriod: paymentPeriod.value,
    concurrency: concurrency.value,
  }));
}

const useCustomAddress = ref(false);
const customAddress = ref({
  name: '',
  line1: '',
  city: '',
  state: '',
  postal_code: '',
  country: 'US',
});

const captchaConfig = ref({
  api_url: 'https://api.yescaptcha.com',
  api_key: '',
});

const proxyConfig = ref({
  host: '',
  port: 10808,
  user: '',
  pass: '',
});

const teamsTier = ref(2);
const paymentPeriod = ref(1);
const concurrency = ref(1);

const groups = computed(() => settingsStore.groups || []);
const filteredGroupAccounts = computed(() => {
  if (selectedGroup.value === '__all__') return accountsStore.accounts;
  return accountsStore.accounts.filter(a => a.group === selectedGroup.value);
});

const paginatedAccounts = computed(() => {
  const start = (currentPage.value - 1) * pageSize.value;
  return filteredGroupAccounts.value.slice(start, start + pageSize.value);
});

function handleGroupChange() {
  selectedAccountIds.value = [];
  currentPage.value = 1;
}

function handlePageSizeChange(size: number) {
  pageSize.value = size;
  currentPage.value = 1;
}

function selectAllInGroup() {
  selectedAccountIds.value = filteredGroupAccounts.value.map(a => a.id);
}

function selectFreeOnly() {
  selectedAccountIds.value = filteredGroupAccounts.value
    .filter(a => a.plan_name === 'Free')
    .map(a => a.id);
}

function clearAccountSelection() {
  selectedAccountIds.value = [];
}

async function refreshAccounts() {
  if (selectedAccountIds.value.length === 0) {
    ElMessage.warning('请先选择要刷新的账号');
    return;
  }
  isRefreshing.value = true;
  try {
    const selected = accountsStore.accounts.filter(a => selectedAccountIds.value.includes(a.id));
    const result = await accountsStore.batchRefreshTokens(selected);
    ElMessage.success(`已刷新 ${result.success}/${result.total} 个账号状态`);
  } catch (e: any) {
    ElMessage.error(`刷新失败: ${e}`);
  } finally {
    isRefreshing.value = false;
  }
}


const canStart = computed(() => {
  return selectedAccountIds.value.length > 0
    && selectedCardIndices.value.length > 0
    && !isRunning.value;
});

// ─── 执行 Tab ────────────────────────────────────
const isRunning = ref(false);
const currentBatchId = ref('');

interface TaskItem {
  account_id: string;
  email: string;
  status: string;
  step: number;
  step_name: string;
  error: string | null;
}

const taskList = ref<TaskItem[]>([]);

const taskStats = computed(() => {
  const list = taskList.value;
  return {
    success: list.filter(t => t.status === 'success').length,
    failed: list.filter(t => t.status === 'failed').length,
    running: list.filter(t => t.status === 'running').length,
    pending: list.filter(t => t.status === 'pending').length,
  };
});

const overallProgress = computed(() => {
  if (taskList.value.length === 0) return 0;
  return Math.round(((taskStats.value.success + taskStats.value.failed) / taskList.value.length) * 100);
});

function taskTagType(status: string) {
  switch (status) {
    case 'success': return 'success';
    case 'failed': return 'danger';
    case 'running': return 'primary';
    case 'cancelled': return 'warning';
    default: return 'info';
  }
}

function taskStatusText(status: string) {
  switch (status) {
    case 'success': return '成功';
    case 'failed': return '失败';
    case 'pending': return '等待中';
    case 'cancelled': return '已取消';
    case 'running': return '进行中';
    default: return status;
  }
}

// ─── Turnstile 批量验证 ─────────────────────
const showTurnstile = ref(false);
const turnstileAccounts = ref<{ id: string; email: string }[]>([]);
const collectedTurnstileTokens = ref<Record<string, string>>({});

function startBind() {
  if (!canStart.value) return;

  // 构建账号列表用于 Turnstile 验证
  turnstileAccounts.value = selectedAccountIds.value.map(id => {
    const acc = accountsStore.accounts.find(a => a.id === id);
    return { id, email: acc?.email || id };
  });
  collectedTurnstileTokens.value = {};

  // 弹出批量人机验证
  showTurnstile.value = true;
}

function onTurnstileCompleted(results: { accountId: string; email: string; token?: string; error?: string }[]) {
  showTurnstile.value = false;

  // 收集成功的 token
  const tokens: Record<string, string> = {};
  const failedAccounts: string[] = [];
  for (const r of results) {
    if (r.token) {
      tokens[r.accountId] = r.token;
    } else {
      failedAccounts.push(r.email);
    }
  }

  if (Object.keys(tokens).length === 0) {
    ElMessage.error('所有账号的人机验证均失败，无法继续');
    return;
  }

  if (failedAccounts.length > 0) {
    ElMessage.warning(`${failedAccounts.length} 个账号验证失败，将跳过: ${failedAccounts.join(', ')}`);
  }

  collectedTurnstileTokens.value = tokens;
  // 验证完成，自动开始绑卡
  doStartBind(Object.keys(tokens));
}

function onTurnstileClosed() {
  showTurnstile.value = false;
}

async function doStartBind(verifiedAccountIds: string[]) {
  isRunning.value = true;
  activeTab.value = 'execution';
  logs.value = [];

  // 初始化任务列表
  taskList.value = verifiedAccountIds.map(id => {
    const acc = accountsStore.accounts.find(a => a.id === id);
    return {
      account_id: id,
      email: acc?.email || id,
      status: 'pending',
      step: 0,
      step_name: '等待中',
      error: null,
    };
  });

  try {
    const proxy = proxyConfig.value.host ? {
      host: proxyConfig.value.host || null,
      port: proxyConfig.value.port || null,
      user: proxyConfig.value.user || null,
      pass: proxyConfig.value.pass || null,
    } : null;

    const selectedCards = selectedCardIndices.value
      .sort((a, b) => a - b)
      .map(i => ({
        number: savedCards.value[i].number,
        cvc: savedCards.value[i].cvc,
        exp_year: savedCards.value[i].exp_year,
        exp_month: savedCards.value[i].exp_month,
      }));

    persistConfig();

    const result = await invoke<{ success: boolean; batch_id: string; total: number }>('stripe_bind_start', {
      request: {
        account_ids: verifiedAccountIds,
        cards: selectedCards,
        captcha: captchaConfig.value,
        proxy: proxy,
        teams_tier: teamsTier.value,
        payment_period: paymentPeriod.value,
        concurrency: concurrency.value,
        custom_address: useCustomAddress.value ? {
          country: customAddress.value.country || 'US',
          line1: customAddress.value.line1,
          city: customAddress.value.city,
          state: customAddress.value.state,
          postal_code: customAddress.value.postal_code,
        } : null,
        custom_name: useCustomAddress.value && customAddress.value.name ? customAddress.value.name : null,
        turnstile_tokens: collectedTurnstileTokens.value,
      },
    });

    currentBatchId.value = result.batch_id;
    ElMessage.success(`批量绑卡任务已启动，共 ${result.total} 个账号`);
  } catch (e: any) {
    ElMessage.error(`启动失败: ${e}`);
    isRunning.value = false;
  }
}

async function cancelBind() {
  if (!currentBatchId.value) return;
  try {
    await invoke('stripe_bind_cancel', { batchId: currentBatchId.value });
    ElMessage.warning('已取消剩余任务');
  } catch (e: any) {
    ElMessage.error(`取消失败: ${e}`);
  }
}

// ─── 日志 Tab ────────────────────────────────────
interface LogEntry {
  task_id: string;
  level: string;
  time: string;
  message: string;
}

const logs = ref<LogEntry[]>([]);
const logFilter = ref('');
const autoScroll = ref(true);
const logContainerRef = ref<HTMLElement | null>(null);

const filteredLogs = computed(() => {
  if (!logFilter.value) return logs.value;
  const q = logFilter.value.toLowerCase();
  return logs.value.filter(l => l.message.toLowerCase().includes(q));
});

function clearLogs() {
  logs.value = [];
}

function exportLogs() {
  const text = logs.value.map(l => `[${l.time}] [${l.level}] ${l.message}`).join('\n');
  const blob = new Blob([text], { type: 'text/plain' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `stripe_bind_log_${new Date().toISOString().slice(0, 10)}.txt`;
  a.click();
  URL.revokeObjectURL(url);
}

function scrollToBottom() {
  if (autoScroll.value && logContainerRef.value) {
    nextTick(() => {
      logContainerRef.value!.scrollTop = logContainerRef.value!.scrollHeight;
    });
  }
}

// ─── Tauri 事件监听 ──────────────────────────────
let unlistenLog: UnlistenFn | null = null;
let unlistenProgress: UnlistenFn | null = null;
let unlistenTaskDone: UnlistenFn | null = null;
let unlistenBatchDone: UnlistenFn | null = null;

onMounted(async () => {
  // 加载保存的卡片和配置
  loadSavedCards();
  loadConfig();

  unlistenLog = await listen<LogEntry>('stripe-bind-log', (event) => {
    logs.value.push(event.payload);
    if (logs.value.length > 5000) {
      logs.value = logs.value.slice(-3000);
    }
    scrollToBottom();
  });

  unlistenProgress = await listen<{
    task_id: string; account_id: string; step: number; step_name: string; status: string;
  }>('stripe-bind-progress', (event) => {
    const p = event.payload;
    const task = taskList.value.find(t => t.account_id === p.account_id);
    if (task) {
      task.step = p.step;
      task.step_name = p.step_name;
      task.status = p.status;
    }
  });

  unlistenTaskDone = await listen<{
    task_id: string; account_id: string; email: string; status: string; error: string | null;
  }>('stripe-bind-task-done', (event) => {
    const p = event.payload;
    const task = taskList.value.find(t => t.account_id === p.account_id);
    if (task) {
      task.status = p.status;
      task.error = p.error || null;
    }
  });

  unlistenBatchDone = await listen<{ task_id: string }>('stripe-bind-batch-done', (_event) => {
    isRunning.value = false;
    ElMessage.success('所有绑卡任务已完成');
  });
});

onUnmounted(() => {
  unlistenLog?.();
  unlistenProgress?.();
  unlistenTaskDone?.();
  unlistenBatchDone?.();
});

function handleClose() {
  // 不清理状态，允许查看日志
}
</script>

<style scoped>
.account-select-box {
  max-height: 200px;
  overflow-y: auto;
  border: 1px solid var(--el-border-color);
  border-radius: 4px;
  padding: 8px;
  width: 100%;
}

.account-checkbox {
  display: block;
  margin-bottom: 4px;
  margin-left: 0 !important;
}

.acc-email {
  font-size: 13px;
  font-family: monospace;
}

.account-pagination {
  margin-top: 8px;
  display: flex;
  justify-content: flex-end;
  width: 100%;
}

/* 卡片管理 */
.card-list-box {
  max-height: 160px;
  overflow-y: auto;
  border: 1px solid var(--el-border-color);
  border-radius: 4px;
  padding: 6px 8px;
  width: 100%;
}

.card-item {
  display: flex;
  align-items: center;
  padding: 4px 0;
  border-bottom: 1px solid var(--el-border-color-lighter);
}

.card-item:last-child {
  border-bottom: none;
}

.card-number {
  font-family: monospace;
  font-weight: bold;
  margin-right: 8px;
}

.card-meta {
  font-size: 12px;
  color: #909399;
}

/* 执行页 */
.task-list-container {
  max-height: 400px;
  overflow-y: auto;
}

.task-item {
  display: flex;
  align-items: center;
  padding: 8px 12px;
  border-bottom: 1px solid var(--el-border-color-lighter);
  gap: 12px;
}

.task-item:last-child {
  border-bottom: none;
}

.task-email {
  flex: 1;
  font-family: monospace;
  font-size: 13px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.task-step {
  flex-shrink: 0;
}

.task-error {
  font-size: 11px;
  color: #f56c6c;
  max-width: 200px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.task-success { background-color: rgba(103, 194, 58, 0.05); }
.task-failed { background-color: rgba(245, 108, 108, 0.05); }
.task-running { background-color: rgba(64, 158, 255, 0.05); }

/* 日志页 */
.log-container {
  height: 450px;
  overflow-y: auto;
  background: #1e1e1e;
  border-radius: 4px;
  padding: 8px;
  font-family: 'Cascadia Code', 'Fira Code', 'Consolas', monospace;
  font-size: 12px;
  line-height: 1.6;
}

.log-line {
  white-space: pre-wrap;
  word-break: break-all;
}

.log-time {
  color: #6a9955;
  margin-right: 6px;
}

.log-level {
  margin-right: 6px;
  font-weight: bold;
}

.log-msg {
  color: #d4d4d4;
}

.log-info .log-level { color: #569cd6; }
.log-warn .log-level { color: #ce9178; }
.log-warn .log-msg { color: #ce9178; }
.log-error .log-level { color: #f44747; }
.log-error .log-msg { color: #f44747; }
.log-debug .log-level { color: #808080; }
.log-debug .log-msg { color: #808080; }

/* 深色模式适配 */
:deep(.el-tabs--border-card) {
  border-color: var(--el-border-color);
}

html.dark .account-select-box,
html.dark .card-list-box {
  border-color: #4c4d4f;
  background-color: #262729;
}

html.dark .card-item {
  border-bottom-color: #4c4d4f;
}

html.dark .task-item {
  border-bottom-color: #4c4d4f;
}

html.dark .task-success { background-color: rgba(103, 194, 58, 0.08); }
html.dark .task-failed { background-color: rgba(245, 108, 108, 0.08); }
html.dark .task-running { background-color: rgba(64, 158, 255, 0.08); }
</style>
