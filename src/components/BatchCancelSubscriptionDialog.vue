<template>
  <el-dialog
    v-model="visible"
    width="700px"
    class="batch-cancel-dialog"
    :close-on-click-modal="false"
    :close-on-press-escape="!isRunning"
    :show-close="!isRunning"
    @close="handleClose"
  >
    <template #header>
      <div class="dialog-header">
        <div class="header-icon">
          <el-icon><CircleClose /></el-icon>
        </div>
        <div class="header-text">
          <h3>批量取消订阅</h3>
          <p>并发执行，快速取消多个账号的当前订阅</p>
        </div>
      </div>
    </template>

    <div class="batch-cancel-content">
      <!-- 选中账号信息 -->
      <div class="selected-accounts-card">
        <div class="card-icon">
          <el-icon><User /></el-icon>
        </div>
        <div class="card-info">
          <span class="label">已选择账号</span>
          <span class="count">{{ selectedAccountIds.length }}</span>
        </div>
      </div>

      <!-- 取消原因选择 -->
      <div class="reason-selection">
        <div class="section-header">
          <el-icon><QuestionFilled /></el-icon>
          <span>选择取消原因</span>
        </div>
        <div class="reason-cards">
          <div
            v-for="reason in cancelReasons"
            :key="reason.value"
            class="reason-card"
            :class="{ active: selectedReason === reason.value, disabled: isRunning }"
            @click="!isRunning && (selectedReason = reason.value)"
          >
            <div class="reason-icon">{{ reason.icon }}</div>
            <div class="reason-label">{{ reason.label }}</div>
            <div class="reason-check" v-if="selectedReason === reason.value">
              <el-icon><Check /></el-icon>
            </div>
          </div>
        </div>
      </div>

      <!-- 警告提示 -->
      <div class="warning-notice">
        <el-icon><Warning /></el-icon>
        <div class="notice-text">
          <span class="notice-title">操作提示</span>
          <span class="notice-desc">取消订阅后，账号将在当前计费周期结束后降级，此操作不可撤销</span>
        </div>
      </div>

      <!-- 执行状态 -->
      <div v-if="isRunning || stats.totalAttempts > 0" class="execution-panel">
        <div class="panel-header">
          <div class="header-left">
            <el-icon v-if="isRunning" class="is-loading"><Loading /></el-icon>
            <el-icon v-else><SuccessFilled /></el-icon>
            <span>{{ isRunning ? '正在执行' : '执行完成' }}</span>
          </div>
          <el-tag
            :type="isRunning ? 'primary' : 'success'"
            effect="dark"
            size="small"
            round
          >
            {{ isRunning ? '运行中' : '已完成' }}
          </el-tag>
        </div>

        <!-- 统计卡片 -->
        <div class="stats-cards">
          <div class="stat-card success">
            <el-icon><SuccessFilled /></el-icon>
            <div class="stat-value">{{ stats.successCount }}</div>
            <div class="stat-label">成功</div>
          </div>
          <div class="stat-card failed">
            <el-icon><CircleCloseFilled /></el-icon>
            <div class="stat-value">{{ stats.failedCount }}</div>
            <div class="stat-label">失败</div>
          </div>
          <div class="stat-card total">
            <el-icon><DataLine /></el-icon>
            <div class="stat-value">{{ stats.totalAttempts }}</div>
            <div class="stat-label">总计</div>
          </div>
          <div class="stat-card progress">
            <el-icon><User /></el-icon>
            <div class="stat-value">{{ stats.processedAccounts }}/{{ selectedAccountIds.length }}</div>
            <div class="stat-label">进度</div>
          </div>
        </div>

        <!-- 最后错误 -->
        <div v-if="stats.lastError" class="error-alert">
          <el-icon><InfoFilled /></el-icon>
          <span>{{ stats.lastError }}</span>
        </div>

        <!-- 执行日志 -->
        <div v-if="executionLogs.length > 0" class="logs-section">
          <div class="logs-header">
            <div class="header-left">
              <el-icon><Document /></el-icon>
              <span>执行日志</span>
              <el-tag size="small" type="info" effect="plain">{{ executionLogs.length }}</el-tag>
            </div>
            <el-button link size="small" @click="executionLogs = []">
              <el-icon><Delete /></el-icon>
              清空
            </el-button>
          </div>
          <div class="logs-container" ref="logsContainer">
            <div
              v-for="(log, index) in executionLogs.slice(-100)"
              :key="index"
              :class="['log-item', log.type]"
            >
              <span class="log-time">{{ log.time }}</span>
              <span class="log-message">{{ log.message }}</span>
            </div>
          </div>
        </div>
      </div>
    </div>

    <template #footer>
      <div class="dialog-footer">
        <el-button @click="handleClose" :disabled="isRunning" size="large">
          取消
        </el-button>
        <el-button
          v-if="isRunning"
          type="warning"
          size="large"
          @click="stopExecution"
        >
          <el-icon><VideoPause /></el-icon>
          停止执行
        </el-button>
        <el-button
          v-else
          type="danger"
          size="large"
          @click="startExecution"
          :disabled="selectedAccountIds.length === 0"
        >
          <el-icon><CircleClose /></el-icon>
          开始批量取消
        </el-button>
      </div>
    </template>
  </el-dialog>
</template>

<script setup lang="ts">
import { ref, reactive, watch, nextTick } from 'vue';
import { ElMessage } from 'element-plus';
import {
  User, SuccessFilled, CircleCloseFilled, DataLine,
  InfoFilled, Loading, VideoPause, CircleClose,
  Document, Delete, Warning, QuestionFilled, Check
} from '@element-plus/icons-vue';
import { apiService } from '@/api';
import type { Account } from '@/types';

const props = defineProps<{
  modelValue: boolean;
  selectedAccountIds: string[];
  accounts: Account[];
}>();

const emit = defineEmits<{
  'update:modelValue': [value: boolean];
  'success': [];
  'close': [];
}>();

const visible = ref(props.modelValue);

const cancelReasons = [
  { value: 'too_expensive', label: '价格太贵', icon: '💸' },
  { value: 'not_using', label: '不常使用', icon: '😴' },
  { value: 'missing_features', label: '缺少功能', icon: '🔧' },
  { value: 'switching_service', label: '切换服务', icon: '🔄' },
  { value: 'other', label: '其他原因', icon: '📝' },
];

const selectedReason = ref('other');
const isRunning = ref(false);
const shouldStop = ref(false);
const logsContainer = ref<HTMLElement | null>(null);

const stats = reactive({
  successCount: 0,
  failedCount: 0,
  totalAttempts: 0,
  processedAccounts: 0,
  lastError: ''
});

interface LogEntry {
  time: string;
  message: string;
  type: 'success' | 'error' | 'info';
}

const executionLogs = ref<LogEntry[]>([]);

watch(() => props.modelValue, (val) => {
  visible.value = val;
  if (val) {
    resetState();
  }
});

watch(visible, (val) => {
  emit('update:modelValue', val);
});

function resetState() {
  selectedReason.value = 'other';
  isRunning.value = false;
  shouldStop.value = false;
  stats.successCount = 0;
  stats.failedCount = 0;
  stats.totalAttempts = 0;
  stats.processedAccounts = 0;
  stats.lastError = '';
  executionLogs.value = [];
}

function addLog(message: string, type: 'success' | 'error' | 'info') {
  const now = new Date();
  const time = `${now.getHours().toString().padStart(2, '0')}:${now.getMinutes().toString().padStart(2, '0')}:${now.getSeconds().toString().padStart(2, '0')}`;
  executionLogs.value.push({ time, message, type });

  nextTick(() => {
    if (logsContainer.value) {
      logsContainer.value.scrollTop = logsContainer.value.scrollHeight;
    }
  });
}

function getSelectedAccounts(): Account[] {
  return props.accounts.filter(a => props.selectedAccountIds.includes(a.id));
}

async function executeSingleCancel(account: Account): Promise<{ success: boolean; error?: string }> {
  try {
    const result = await apiService.cancelSubscription(account.id, selectedReason.value);
    if (result.success) {
      return { success: true };
    } else {
      return { success: false, error: result.raw_response || '取消订阅失败' };
    }
  } catch (err: any) {
    return { success: false, error: err.toString() };
  }
}

async function startExecution() {
  const selectedAccounts = getSelectedAccounts();
  if (selectedAccounts.length === 0) {
    ElMessage.warning('没有选中的账号');
    return;
  }

  isRunning.value = true;
  shouldStop.value = false;
  stats.successCount = 0;
  stats.failedCount = 0;
  stats.totalAttempts = 0;
  stats.processedAccounts = 0;
  stats.lastError = '';

  const reasonLabel = cancelReasons.find(r => r.value === selectedReason.value)?.label || selectedReason.value;
  addLog(`开始批量取消订阅，原因：${reasonLabel}（${selectedAccounts.length} 个账号）`, 'info');

  const tasks = selectedAccounts.map(async (account) => {
    if (shouldStop.value) return;

    stats.totalAttempts++;
    const result = await executeSingleCancel(account);

    if (result.success) {
      stats.successCount++;
      addLog(`[${account.email}] 取消订阅成功`, 'success');
    } else {
      stats.failedCount++;
      stats.lastError = result.error || '未知错误';
      addLog(`[${account.email}] 取消订阅失败: ${result.error}`, 'error');
    }

    stats.processedAccounts++;
  });

  await Promise.all(tasks);

  isRunning.value = false;

  if (stats.successCount > 0) {
    ElMessage.success(`批量取消完成: 成功 ${stats.successCount} 次，失败 ${stats.failedCount} 次`);
    emit('success');
  } else if (stats.totalAttempts > 0) {
    ElMessage.error('批量取消失败，没有成功的操作');
  }
}

function stopExecution() {
  shouldStop.value = true;
  ElMessage.info('正在停止执行...');
}

function handleClose() {
  if (isRunning.value) {
    return;
  }
  visible.value = false;
  emit('close');
}
</script>

<style scoped lang="scss">
.dialog-header {
  display: flex;
  align-items: center;
  gap: 16px;

  .header-icon {
    width: 48px;
    height: 48px;
    border-radius: 12px;
    background: linear-gradient(135deg, #ef4444 0%, #dc2626 100%);
    display: flex;
    align-items: center;
    justify-content: center;

    .el-icon {
      font-size: 24px;
      color: #fff;
    }
  }

  .header-text {
    h3 {
      margin: 0 0 4px 0;
      font-size: 18px;
      font-weight: 600;
      color: #1e293b;
    }

    p {
      margin: 0;
      font-size: 13px;
      color: #64748b;
    }
  }
}

.batch-cancel-content {
  padding: 4px 0;
}

.selected-accounts-card {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 16px 20px;
  background: linear-gradient(135deg, #fff1f2 0%, #ffe4e6 100%);
  border-radius: 12px;
  margin-bottom: 24px;
  position: relative;
  overflow: hidden;

  &::before {
    content: '';
    position: absolute;
    top: -50%;
    right: -20%;
    width: 200px;
    height: 200px;
    background: radial-gradient(circle, rgba(239, 68, 68, 0.15) 0%, transparent 70%);
    border-radius: 50%;
  }

  .card-icon {
    width: 44px;
    height: 44px;
    border-radius: 10px;
    background: linear-gradient(135deg, #ef4444 0%, #dc2626 100%);
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: 0 4px 12px rgba(239, 68, 68, 0.3);

    .el-icon {
      font-size: 22px;
      color: #fff;
    }
  }

  .card-info {
    display: flex;
    flex-direction: column;
    gap: 2px;

    .label {
      font-size: 12px;
      color: #64748b;
    }

    .count {
      font-size: 28px;
      font-weight: 700;
      color: #b91c1c;
      line-height: 1;
    }
  }
}

.reason-selection {
  margin-bottom: 20px;

  .section-header {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 14px;
    font-weight: 600;
    color: #374151;
    margin-bottom: 16px;

    .el-icon {
      font-size: 18px;
      color: #ef4444;
    }
  }

  .reason-cards {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: 10px;
  }

  .reason-card {
    position: relative;
    padding: 14px 8px;
    border: 2px solid #e5e7eb;
    border-radius: 10px;
    cursor: pointer;
    transition: all 0.2s ease;
    text-align: center;
    background: #fff;

    &:hover:not(.disabled) {
      border-color: #fca5a5;
      background: #fff5f5;
      transform: translateY(-2px);
    }

    &.active {
      border-color: #ef4444;
      background: linear-gradient(135deg, #fff1f2 0%, #ffe4e6 100%);

      .reason-label {
        color: #dc2626;
        font-weight: 600;
      }
    }

    &.disabled {
      opacity: 0.6;
      cursor: not-allowed;
    }

    .reason-icon {
      font-size: 22px;
      margin-bottom: 6px;
    }

    .reason-label {
      font-size: 11px;
      color: #374151;
      line-height: 1.3;
    }

    .reason-check {
      position: absolute;
      top: 6px;
      right: 6px;
      width: 18px;
      height: 18px;
      border-radius: 50%;
      background: #ef4444;
      display: flex;
      align-items: center;
      justify-content: center;

      .el-icon {
        font-size: 12px;
        color: #fff;
      }
    }
  }
}

.warning-notice {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 14px 16px;
  background: linear-gradient(135deg, #fef3c7 0%, #fde68a 100%);
  border: 1px solid #fcd34d;
  border-radius: 10px;
  margin-bottom: 20px;

  .el-icon {
    font-size: 20px;
    color: #d97706;
    flex-shrink: 0;
    margin-top: 1px;
  }

  .notice-text {
    display: flex;
    flex-direction: column;
    gap: 3px;

    .notice-title {
      font-size: 13px;
      font-weight: 600;
      color: #92400e;
    }

    .notice-desc {
      font-size: 12px;
      color: #78350f;
    }
  }
}

.execution-panel {
  background: #f8fafc;
  border: 1px solid #e2e8f0;
  border-radius: 12px;
  padding: 20px;

  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 16px;

    .header-left {
      display: flex;
      align-items: center;
      gap: 10px;
      font-size: 15px;
      font-weight: 600;
      color: #1e293b;

      .el-icon {
        font-size: 20px;
        color: #ef4444;
      }

      .is-loading {
        animation: rotating 1s linear infinite;
      }
    }
  }

  .stats-cards {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 12px;
    margin-bottom: 16px;

    .stat-card {
      padding: 14px;
      border-radius: 10px;
      text-align: center;

      .el-icon {
        font-size: 20px;
        margin-bottom: 6px;
      }

      .stat-value {
        font-size: 22px;
        font-weight: 700;
        line-height: 1.2;
      }

      .stat-label {
        font-size: 11px;
        margin-top: 2px;
      }

      &.success {
        background: linear-gradient(135deg, #dcfce7 0%, #bbf7d0 100%);
        .el-icon, .stat-value { color: #16a34a; }
        .stat-label { color: #15803d; }
      }

      &.failed {
        background: linear-gradient(135deg, #fee2e2 0%, #fecaca 100%);
        .el-icon, .stat-value { color: #dc2626; }
        .stat-label { color: #b91c1c; }
      }

      &.total {
        background: linear-gradient(135deg, #dbeafe 0%, #bfdbfe 100%);
        .el-icon, .stat-value { color: #2563eb; }
        .stat-label { color: #1d4ed8; }
      }

      &.progress {
        background: linear-gradient(135deg, #ede9fe 0%, #ddd6fe 100%);
        .el-icon, .stat-value { color: #7c3aed; }
        .stat-label { color: #6d28d9; }
      }
    }
  }

  .error-alert {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 12px 16px;
    background: linear-gradient(135deg, #fee2e2 0%, #fecaca 100%);
    border-radius: 8px;
    margin-bottom: 12px;
    font-size: 12px;
    color: #991b1b;
    word-break: break-all;

    .el-icon {
      font-size: 16px;
      color: #dc2626;
      flex-shrink: 0;
      margin-top: 1px;
    }
  }

  .logs-section {
    .logs-header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      margin-bottom: 10px;

      .header-left {
        display: flex;
        align-items: center;
        gap: 8px;
        font-size: 13px;
        font-weight: 500;
        color: #64748b;

        .el-icon {
          font-size: 16px;
        }
      }
    }

    .logs-container {
      max-height: 180px;
      overflow-y: auto;
      background: linear-gradient(135deg, #1e293b 0%, #0f172a 100%);
      border-radius: 10px;
      padding: 12px 16px;
      font-family: 'JetBrains Mono', 'Consolas', 'Monaco', monospace;
      font-size: 12px;

      &::-webkit-scrollbar {
        width: 6px;
      }

      &::-webkit-scrollbar-track {
        background: transparent;
      }

      &::-webkit-scrollbar-thumb {
        background: #475569;
        border-radius: 3px;
      }

      .log-item {
        display: flex;
        gap: 12px;
        padding: 4px 0;
        border-bottom: 1px solid rgba(255, 255, 255, 0.05);

        &:last-child {
          border-bottom: none;
        }

        .log-time {
          color: #64748b;
          flex-shrink: 0;
          font-size: 11px;
        }

        .log-message {
          word-break: break-all;
          line-height: 1.5;
        }

        &.success .log-message {
          color: #4ade80;
        }

        &.error .log-message {
          color: #f87171;
        }

        &.info .log-message {
          color: #60a5fa;
        }
      }
    }
  }
}

@keyframes rotating {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}

.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 12px;
  padding-top: 8px;
}

/* 暗色主题 */
:global(.dark) {
  .dialog-header {
    .header-text {
      h3 { color: #f1f5f9; }
      p { color: #94a3b8; }
    }
  }

  .selected-accounts-card {
    background: linear-gradient(135deg, #450a0a 0%, #7f1d1d 100%);

    .card-info {
      .label { color: #94a3b8; }
      .count { color: #fca5a5; }
    }
  }

  .reason-selection {
    .section-header {
      color: #e2e8f0;
    }

    .reason-card {
      background: #1e293b;
      border-color: #334155;

      &:hover:not(.disabled) {
        background: #334155;
        border-color: #fca5a5;
      }

      &.active {
        background: linear-gradient(135deg, #450a0a 0%, #7f1d1d 100%);
        border-color: #ef4444;
      }

      .reason-label { color: #f1f5f9; }
    }
  }

  .warning-notice {
    background: linear-gradient(135deg, #78350f 0%, #92400e 100%);
    border-color: #b45309;

    .notice-title { color: #fef3c7; }
    .notice-desc { color: #fde68a; }
  }

  .execution-panel {
    background: #1e293b;
    border-color: #334155;

    .panel-header .header-left {
      color: #f1f5f9;
    }

    .stats-cards .stat-card {
      &.success { background: linear-gradient(135deg, #14532d 0%, #166534 100%); }
      &.failed { background: linear-gradient(135deg, #7f1d1d 0%, #991b1b 100%); }
      &.total { background: linear-gradient(135deg, #1e3a8a 0%, #1d4ed8 100%); }
      &.progress { background: linear-gradient(135deg, #4c1d95 0%, #5b21b6 100%); }
    }

    .error-alert {
      background: linear-gradient(135deg, #7f1d1d 0%, #991b1b 100%);
      color: #fecaca;
    }
  }
}
</style>
