<template>
  <el-dialog
    v-model="dialogVisible"
    title="批量人机验证"
    width="95vw"
    style="max-width: 1400px;"
    :close-on-click-modal="false"
    :close-on-press-escape="false"
    :append-to-body="true"
    :show-close="true"
    @close="handleClose"
  >
    <div class="concurrent-turnstile-container">
      <!-- 顶部设置栏 -->
      <div class="settings-bar">
        <div class="settings-left">
          <span class="settings-label">并发数:</span>
          <el-select
            v-model="concurrencyCount"
            size="small"
            style="width: 80px;"
            :disabled="false"
            @change="handleConcurrencyChange"
          >
            <el-option v-for="n in 20" :key="n" :value="n" :label="n" />
          </el-select>
          <span class="settings-tip">（同时显示多个验证框，可手动点击完成验证）</span>
        </div>
        <div class="settings-right">
          <el-tag type="info" size="small">
            待验证: {{ pendingCount }}
          </el-tag>
          <el-tag type="success" size="small">
            已完成: {{ completedCount }}
          </el-tag>
          <el-tag v-if="failedCount > 0" type="danger" size="small">
            失败: {{ failedCount }}
          </el-tag>
        </div>
      </div>

      <!-- 进度条 -->
      <div class="progress-section">
        <el-progress 
          :percentage="progressPercentage" 
          :stroke-width="10"
          :show-text="true"
          :status="progressStatus"
        />
      </div>

      <!-- 验证框网格 - 只显示正在进行的验证 -->
      <div class="turnstile-grid-wrapper">
        <div class="turnstile-grid" :style="{ gridTemplateColumns: `repeat(${Math.min(concurrencyCount, 5)}, 1fr)` }">
          <div
            v-for="slot in pendingSlots"
            :key="slot.id"
            v-memo="[slot.id, slot.status]"
            class="turnstile-slot is-loading"
          >
            <div class="slot-header">
              <span class="slot-email" :title="slot.email">{{ slot.email }}</span>
              <el-tag :type="getStatusTagType(slot.status)" size="small">
                {{ getStatusText(slot.status) }}
              </el-tag>
            </div>
            <div class="slot-content">
              <div 
                :ref="el => setTurnstileRef(slot.id, el as HTMLElement)"
                class="turnstile-widget"
              ></div>
            </div>
          </div>
          <!-- 失败的验证单独显示 -->
          <div
            v-for="slot in errorSlots"
            :key="slot.id"
            v-memo="[slot.id, slot.status]"
            class="turnstile-slot is-error"
          >
            <div class="slot-header">
              <span class="slot-email" :title="slot.email">{{ slot.email }}</span>
              <el-tag type="danger" size="small">验证失败</el-tag>
            </div>
            <div class="slot-content">
              <div class="status-icon error">
                <el-icon :size="32"><CircleClose /></el-icon>
                <el-button type="primary" link size="small" @click="retrySlot(slot)">
                  重试
                </el-button>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- 空状态 -->
      <div v-if="activeSlots.length === 0 && !isProcessing" class="empty-state">
        <el-icon :size="48" color="#c0c4cc"><Warning /></el-icon>
        <p>没有待验证的账号</p>
      </div>
    </div>

    <template #footer>
      <div class="dialog-footer">
        <el-button @click="handleClose" :disabled="isProcessing">
          {{ isProcessing ? '验证中...' : '关闭' }}
        </el-button>
        <el-button 
          v-if="failedCount > 0 && !isProcessing"
          type="warning" 
          @click="retryAllFailed"
        >
          重试失败项 ({{ failedCount }})
        </el-button>
        <el-button 
          v-if="failedCount > 0 && !isProcessing"
          type="primary" 
          @click="finishVerification"
        >
          继续到结果页
        </el-button>
      </div>
    </template>
  </el-dialog>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick, onUnmounted } from 'vue';
import { CircleClose, Warning } from '@element-plus/icons-vue';
import logger from '@/utils/logger';

const TURNSTILE_SITE_KEY = '0x4AAAAAAA447Bur1xJStKg5';

interface AccountItem {
  id: string;
  email: string;
}

interface TurnstileSlot {
  id: string;
  accountId: string;
  email: string;
  status: 'pending' | 'loading' | 'success' | 'error';
  token?: string;
  widgetId?: string;
}

const props = withDefaults(defineProps<{
  visible: boolean;
  accounts: AccountItem[];
  initialConcurrency?: number;
}>(), {
  initialConcurrency: 4
});

const emit = defineEmits<{
  (e: 'update:visible', value: boolean): void;
  (e: 'verified', accountId: string, token: string): void;
  (e: 'failed', accountId: string, error: string): void;
  (e: 'allCompleted', results: { accountId: string; email: string; token?: string; error?: string }[]): void;
  (e: 'close'): void;
}>();

const dialogVisible = ref(false);
const concurrencyCount = ref(props.initialConcurrency);
const activeSlots = ref<TurnstileSlot[]>([]);
const completedResults = ref<{ accountId: string; email: string; token?: string; error?: string }[]>([]);
const turnstileRefs = ref<Map<string, HTMLElement>>(new Map());
const isProcessing = ref(false);
const accountQueue = ref<AccountItem[]>([]);

const pendingSlots = computed(() => activeSlots.value.filter(s => s.status === 'pending' || s.status === 'loading'));
const errorSlots = computed(() => activeSlots.value.filter(s => s.status === 'error'));
const pendingCount = computed(() => accountQueue.value.length + pendingSlots.value.length);
const completedCount = computed(() => completedResults.value.filter(r => r.token).length);
const failedCount = computed(() => completedResults.value.filter(r => r.error).length + errorSlots.value.length);
const progressPercentage = computed(() => {
  const total = props.accounts.length;
  if (total === 0) return 0;
  return Math.round((completedResults.value.length / total) * 100);
});
const progressStatus = computed(() => {
  if (failedCount.value > 0 && completedCount.value === 0) return 'exception';
  if (progressPercentage.value === 100) return 'success';
  return undefined;
});

watch(() => props.visible, (val) => {
  dialogVisible.value = val;
  if (val) {
    initializeVerification();
  } else {
    cleanup();
  }
});

watch(dialogVisible, (val) => {
  emit('update:visible', val);
});

function setTurnstileRef(slotId: string, el: HTMLElement | null) {
  if (el) {
    // 检查是否已经渲染过
    const existingRef = turnstileRefs.value.get(slotId);
    if (existingRef === el) {
      // 相同的元素，不需要重新渲染
      return;
    }
    
    turnstileRefs.value.set(slotId, el);
    
    // 检查 slot 是否已经有 widgetId（已渲染）
    const slot = activeSlots.value.find(s => s.id === slotId);
    if (slot && slot.widgetId) {
      // 已经渲染过，不需要重新渲染
      return;
    }
    
    nextTick(() => {
      renderTurnstileForSlot(slotId);
    });
  } else {
    turnstileRefs.value.delete(slotId);
  }
}

function initializeVerification() {
  isProcessing.value = true;
  completedResults.value = [];
  activeSlots.value = [];
  accountQueue.value = [...props.accounts];
  // 使用传入的初始并发数
  concurrencyCount.value = props.initialConcurrency;
  
  fillSlots();
}

function fillSlots() {
  // 计算当前正在进行中的验证数量（pending 或 loading 状态）
  const activeCount = activeSlots.value.filter(s => s.status === 'pending' || s.status === 'loading').length;
  const availableSlots = concurrencyCount.value - activeCount;
  
  // 添加新的 slot 到数组末尾，不会影响已有的 slot
  for (let i = 0; i < availableSlots && accountQueue.value.length > 0; i++) {
    const account = accountQueue.value.shift()!;
    const slot: TurnstileSlot = {
      id: `slot-${account.id}-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
      accountId: account.id,
      email: account.email,
      status: 'pending'
    };
    activeSlots.value.push(slot);
  }
  
  // 检查是否所有验证都完成
  const allCompleted = accountQueue.value.length === 0 && 
    activeSlots.value.every(s => s.status === 'success' || s.status === 'error');
  
  if (allCompleted) {
    // 有失败项时不自动完成，让用户决定是否重试
    const hasFailures = activeSlots.value.some(s => s.status === 'error') || 
      completedResults.value.some(r => r.error);
    if (hasFailures) {
      // 停止处理状态，让用户可以点击重试或继续
      isProcessing.value = false;
    } else {
      finishVerification();
    }
  }
}

async function loadTurnstileScript(): Promise<void> {
  return new Promise((resolve, reject) => {
    if ((window as any).turnstile) {
      resolve();
      return;
    }
    
    const existingScript = document.querySelector('script[src*="turnstile"]');
    if (existingScript) {
      const checkTurnstile = () => {
        if ((window as any).turnstile) {
          resolve();
        } else {
          setTimeout(checkTurnstile, 100);
        }
      };
      checkTurnstile();
      return;
    }
    
    const script = document.createElement('script');
    script.src = 'https://challenges.cloudflare.com/turnstile/v0/api.js?render=explicit';
    script.async = true;
    script.defer = true;
    
    script.onload = () => {
      const checkTurnstile = () => {
        if ((window as any).turnstile) {
          resolve();
        } else {
          setTimeout(checkTurnstile, 100);
        }
      };
      checkTurnstile();
    };
    
    script.onerror = () => reject(new Error('Failed to load Turnstile script'));
    document.head.appendChild(script);
  });
}

async function renderTurnstileForSlot(slotId: string) {
  const slot = activeSlots.value.find(s => s.id === slotId);
  if (!slot || slot.status === 'success') return;
  
  const container = turnstileRefs.value.get(slotId);
  if (!container) return;
  
  try {
    await loadTurnstileScript();
    
    const turnstile = (window as any).turnstile;
    if (!turnstile) return;
    
    if (slot.widgetId) {
      try {
        turnstile.remove(slot.widgetId);
      } catch (e) {
        // Ignore
      }
    }
    
    container.innerHTML = '';
    slot.status = 'loading';
    
    slot.widgetId = turnstile.render(container, {
      sitekey: TURNSTILE_SITE_KEY,
      theme: 'light',
      size: 'compact',
      execution: 'render',
      callback: (token: string) => {
        logger.info('ConcurrentTurnstile', `Verification success for ${slot.email}`);
        slot.status = 'success';
        slot.token = token;
        emit('verified', slot.accountId, token);
        
        completedResults.value.push({
          accountId: slot.accountId,
          email: slot.email,
          token
        });
        
        setTimeout(() => {
          removeSlotAndFillNext(slot);
        }, 500);
      },
      'error-callback': () => {
        logger.error('ConcurrentTurnstile', `Verification failed for ${slot.email}`);
        slot.status = 'error';
        emit('failed', slot.accountId, '验证失败');
      },
      'expired-callback': () => {
        logger.warn('ConcurrentTurnstile', `Token expired for ${slot.email}`);
        slot.status = 'pending';
        nextTick(() => {
          renderTurnstileForSlot(slotId);
        });
      }
    });
  } catch (error) {
    logger.error('ConcurrentTurnstile', 'Render error', error);
    slot.status = 'error';
  }
}

function removeSlotAndFillNext(slot: TurnstileSlot) {
  // 只清理 widget，不移除 slot（避免触发数组变化导致其他 slot 重新渲染）
  const turnstile = (window as any).turnstile;
  if (turnstile && slot.widgetId) {
    try {
      turnstile.remove(slot.widgetId);
      slot.widgetId = undefined;
    } catch (e) {
      // Ignore
    }
  }
  
  // 填充新的 slot
  fillSlots();
}

function retrySlot(slot: TurnstileSlot) {
  slot.status = 'pending';
  slot.token = undefined;
  nextTick(() => {
    renderTurnstileForSlot(slot.id);
  });
}

function retryAllFailed() {
  const failedSlots = activeSlots.value.filter(s => s.status === 'error');
  failedSlots.forEach(slot => retrySlot(slot));
  
  const failedResults = completedResults.value.filter(r => r.error);
  failedResults.forEach(result => {
    completedResults.value = completedResults.value.filter(r => r.accountId !== result.accountId);
    accountQueue.value.push({ id: result.accountId, email: result.email });
  });
  
  fillSlots();
}

function finishVerification() {
  isProcessing.value = false;
  emit('allCompleted', completedResults.value);
}

function handleConcurrencyChange() {
  if (isProcessing.value) {
    fillSlots();
  }
}

function cleanup() {
  const turnstile = (window as any).turnstile;
  activeSlots.value.forEach(slot => {
    if (turnstile && slot.widgetId) {
      try {
        turnstile.remove(slot.widgetId);
      } catch (e) {
        // Ignore
      }
    }
  });
  
  activeSlots.value = [];
  accountQueue.value = [];
  turnstileRefs.value.clear();
  isProcessing.value = false;
}

function handleClose() {
  if (isProcessing.value) {
    finishVerification();
  }
  cleanup();
  dialogVisible.value = false;
  emit('close');
}

function getStatusTagType(status: TurnstileSlot['status']) {
  switch (status) {
    case 'success': return 'success';
    case 'error': return 'danger';
    case 'loading': return 'warning';
    default: return 'info';
  }
}

function getStatusText(status: TurnstileSlot['status']) {
  switch (status) {
    case 'success': return '已完成';
    case 'error': return '失败';
    case 'loading': return '验证中';
    default: return '等待中';
  }
}

onUnmounted(() => {
  cleanup();
});
</script>

<style scoped>
.concurrent-turnstile-container {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.settings-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  background: #f5f7fa;
  border-radius: 8px;
}

.settings-left {
  display: flex;
  align-items: center;
  gap: 8px;
}

.settings-label {
  font-weight: 500;
  color: #606266;
}

.settings-tip {
  font-size: 12px;
  color: #909399;
}

.settings-right {
  display: flex;
  gap: 8px;
}

.progress-section {
  padding: 0 4px;
}

.turnstile-grid-wrapper {
  max-height: 60vh;
  overflow-y: auto;
  padding: 4px;
  margin: 0 -4px;
}

.turnstile-grid {
  display: grid;
  gap: 12px;
  min-height: 150px;
}

.turnstile-slot {
  border: 1px solid #ebeef5;
  border-radius: 8px;
  padding: 12px;
  background: #fff;
  transition: all 0.3s;
}

.turnstile-slot.is-loading {
  border-color: #e6a23c;
  background: #fdf6ec;
}

.turnstile-slot.is-success {
  border-color: #67c23a;
  background: #f0f9eb;
}

.turnstile-slot.is-error {
  border-color: #f56c6c;
  background: #fef0f0;
}

.slot-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 10px;
  padding-bottom: 8px;
  border-bottom: 1px solid #ebeef5;
}

.slot-email {
  font-size: 13px;
  font-weight: 500;
  color: #303133;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 150px;
}

.slot-content {
  display: flex;
  justify-content: center;
  align-items: center;
  min-height: 80px;
}

.turnstile-widget {
  transform: scale(0.85);
  transform-origin: center;
}

.status-icon {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
}

.status-icon.success {
  color: #67c23a;
}

.status-icon.error {
  color: #f56c6c;
}

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 40px;
  color: #909399;
}

.empty-state p {
  margin-top: 12px;
  font-size: 14px;
}

.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
}
</style>
