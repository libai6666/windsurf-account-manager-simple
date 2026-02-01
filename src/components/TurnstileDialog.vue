<template>
  <el-dialog
    v-model="dialogVisible"
    title="人机验证"
    width="400px"
    :close-on-click-modal="false"
    :close-on-press-escape="false"
    :append-to-body="true"
    :destroy-on-close="true"
    @close="handleClose"
  >
    <div class="turnstile-container">
      <p class="turnstile-tip">请完成验证以获取试用链接</p>
      
      <div class="turnstile-wrapper">
        <div 
          ref="turnstileRef" 
          class="cf-turnstile"
        ></div>
      </div>
      
      <p v-if="status === 'loading'" class="status-text loading">
        <el-icon class="is-loading"><Loading /></el-icon>
        加载验证中...
      </p>
      <p v-else-if="status === 'success'" class="status-text success">
        <el-icon><CircleCheck /></el-icon>
        验证成功！
      </p>
      <p v-else-if="status === 'error'" class="status-text error">
        <el-icon><CircleClose /></el-icon>
        验证失败，请重试
      </p>
    </div>
    
    <template #footer>
      <el-button @click="handleClose">取消</el-button>
      <el-button 
        type="primary" 
        :disabled="status !== 'success'"
        :loading="isSubmitting"
        @click="handleConfirm"
      >
        获取链接
      </el-button>
    </template>
  </el-dialog>
</template>

<script setup lang="ts">
import { ref, watch, onUnmounted, nextTick } from 'vue';
import { Loading, CircleCheck, CircleClose } from '@element-plus/icons-vue';
import logger from '@/utils/logger';

const TURNSTILE_SITE_KEY = '0x4AAAAAAA447Bur1xJStKg5';

const props = defineProps<{
  visible: boolean;
  autoSubmit?: boolean; // 验证成功后是否自动提交，默认 false
}>();

const emit = defineEmits<{
  (e: 'update:visible', value: boolean): void;
  (e: 'success', token: string): void;
  (e: 'cancel'): void;
}>();

const dialogVisible = ref(false);
const turnstileRef = ref<HTMLElement | null>(null);
const turnstileToken = ref('');
const status = ref<'idle' | 'loading' | 'success' | 'error'>('loading');
const isSubmitting = ref(false);
const widgetId = ref<string | null>(null);
const autoClickTimer = ref<number | null>(null);
const autoClickAttempts = ref(0);
const MAX_AUTO_CLICK_ATTEMPTS = 10;
const isClosing = ref(false);  // 防止重复关闭

// 同步 visible
watch(() => props.visible, (val) => {
  // 打开对话框时，立即重置 isClosing 防止竞态条件
  if (val) {
    isClosing.value = false;
  }
  
  logger.info('TurnstileDialog', `visible changed to ${val}`, { isClosing: isClosing.value });
  dialogVisible.value = val;
  if (val) {
    // 完全重置所有状态
    logger.info('TurnstileDialog', 'Opening dialog, resetting state');
    resetState();
    nextTick(() => {
      loadTurnstile();
    });
  } else {
    // 关闭时清理定时器和widget
    logger.info('TurnstileDialog', 'Closing dialog, cleanup');
    cleanupAll();
  }
});

// 重置所有状态
function resetState() {
  status.value = 'loading';
  turnstileToken.value = '';
  isSubmitting.value = false;
  autoClickAttempts.value = 0;
  isClosing.value = false;  // 重置关闭标志
  clearAutoClickTimer();
}

// 清理自动点击定时器
function clearAutoClickTimer() {
  if (autoClickTimer.value) {
    clearInterval(autoClickTimer.value);
    autoClickTimer.value = null;
  }
}

// 清理所有资源
function cleanupAll() {
  clearAutoClickTimer();
  
  const turnstile = (window as any).turnstile;
  const currentWidgetId = widgetId.value;
  
  // 重置widgetId
  widgetId.value = null;
  
  if (currentWidgetId && turnstile) {
    try {
      turnstile.remove(currentWidgetId);
      console.log('[Turnstile] Widget removed:', currentWidgetId);
    } catch (e) {
      console.log('[Turnstile] Cleanup widget failed:', e);
    }
  }
  
  // 清空容器内容
  if (turnstileRef.value) {
    turnstileRef.value.innerHTML = '';
  }
}

watch(dialogVisible, (val) => {
  emit('update:visible', val);
});

// 加载 Turnstile 脚本
function loadTurnstileScript(): Promise<void> {
  return new Promise((resolve, reject) => {
    if ((window as any).turnstile) {
      resolve();
      return;
    }
    
    const script = document.createElement('script');
    script.src = 'https://challenges.cloudflare.com/turnstile/v0/api.js?render=explicit';
    script.async = true;
    script.defer = true;
    
    script.onload = () => {
      console.log('[Turnstile] Script loaded');
      resolve();
    };
    
    script.onerror = () => {
      console.error('[Turnstile] Failed to load script');
      reject(new Error('Failed to load Turnstile script'));
    };
    
    document.head.appendChild(script);
  });
}

// 渲染 Turnstile 组件
async function loadTurnstile() {
  try {
    await loadTurnstileScript();
    
    // 等待 turnstile 对象可用
    await new Promise<void>((resolve) => {
      const checkTurnstile = () => {
        if ((window as any).turnstile) {
          resolve();
        } else {
          setTimeout(checkTurnstile, 100);
        }
      };
      checkTurnstile();
    });
    
    const turnstile = (window as any).turnstile;
    
    // 如果已有 widget，先移除
    if (widgetId.value) {
      try {
        turnstile.remove(widgetId.value);
      } catch (e) {
        console.log('[Turnstile] Remove old widget failed:', e);
      }
    }
    
    // 清空容器
    if (turnstileRef.value) {
      turnstileRef.value.innerHTML = '';
    }
    
    // 渲染新的 widget
    await nextTick();
    
    if (turnstileRef.value) {
      widgetId.value = turnstile.render(turnstileRef.value, {
        sitekey: TURNSTILE_SITE_KEY,
        theme: 'light',
        execution: 'execute',  // 使用execute模式，允许通过turnstile.execute()触发
        callback: (token: string) => {
          console.log('[Turnstile] Verification success');
          clearAutoClickTimer();
          turnstileToken.value = token;
          status.value = 'success';
          // 只有 autoSubmit 为 true 时才自动触发获取链接
          if (props.autoSubmit) {
            setTimeout(() => {
              if (turnstileToken.value) {
                isSubmitting.value = true;
                emit('success', turnstileToken.value);
              }
            }, 800);
          }
        },
        'error-callback': () => {
          console.error('[Turnstile] Verification failed');
          clearAutoClickTimer();
          status.value = 'error';
        },
        'expired-callback': () => {
          console.log('[Turnstile] Token expired');
          status.value = 'idle';
          turnstileToken.value = '';
          // 重新启动自动点击检测
          startAutoClickDetection();
        }
      });
      
      status.value = 'idle';
      
      // 立即尝试执行验证（widget渲染完成后）
      setTimeout(() => {
        if (widgetId.value && status.value === 'idle') {
          console.log('[Turnstile] Auto-executing after render');
          try {
            turnstile.execute(widgetId.value);
          } catch (e) {
            console.log('[Turnstile] Initial execute failed:', e);
          }
        }
      }, 300);
      
      // 启动自动点击检测（作为备用）
      startAutoClickDetection();
    }
  } catch (error) {
    console.error('[Turnstile] Load error:', error);
    status.value = 'error';
  }
}

function handleConfirm() {
  if (turnstileToken.value) {
    isSubmitting.value = true;
    emit('success', turnstileToken.value);
  }
}

function handleClose() {
  logger.info('TurnstileDialog', 'handleClose called', { isClosing: isClosing.value });
  
  // 防止重复调用
  if (isClosing.value) {
    logger.warn('TurnstileDialog', 'Already closing, skip');
    return;
  }
  isClosing.value = true;
  
  // 清理资源
  try {
    cleanupAll();
  } catch (e) {
    logger.error('TurnstileDialog', 'Cleanup error in handleClose', e);
  }
  
  // 设置状态
  dialogVisible.value = false;
  isSubmitting.value = false;
  
  // 最后发送cancel事件
  logger.info('TurnstileDialog', 'Emitting cancel event');
  emit('cancel');
}

// 自动点击检测和触发
function startAutoClickDetection() {
  clearAutoClickTimer();
  autoClickAttempts.value = 0;
  
  // 使用更短的间隔（500ms）来更快响应
  autoClickTimer.value = window.setInterval(() => {
    // 如果已经验证成功或失败，停止检测
    if (status.value === 'success' || status.value === 'error') {
      clearAutoClickTimer();
      return;
    }
    
    autoClickAttempts.value++;
    
    // 超过最大尝试次数，停止
    if (autoClickAttempts.value > MAX_AUTO_CLICK_ATTEMPTS) {
      console.log('[Turnstile] Max auto-click attempts reached');
      clearAutoClickTimer();
      return;
    }
    
    // 尝试点击验证复选框
    tryClickTurnstileCheckbox();
  }, 500);
}

// 尝试点击Turnstile复选框
function tryClickTurnstileCheckbox() {
  try {
    // 方法1: 尝试使用turnstile.execute()触发验证
    const turnstile = (window as any).turnstile;
    if (turnstile && widgetId.value) {
      try {
        turnstile.execute(widgetId.value);
        console.log('[Turnstile] Execute called');
      } catch (e) {
        console.log('[Turnstile] Execute failed:', e);
      }
    }
    
    // 方法2: 尝试点击iframe内的复选框
    const container = turnstileRef.value;
    if (container) {
      const iframe = container.querySelector('iframe');
      if (iframe) {
        try {
          // 尝试聚焦iframe并模拟点击
          iframe.focus();
          
          // 尝试发送点击事件到iframe
          const iframeRect = iframe.getBoundingClientRect();
          const clickX = iframeRect.left + iframeRect.width / 4;
          const clickY = iframeRect.top + iframeRect.height / 2;
          
          // 创建并分发鼠标事件
          const clickEvent = new MouseEvent('click', {
            bubbles: true,
            cancelable: true,
            clientX: clickX,
            clientY: clickY
          });
          iframe.dispatchEvent(clickEvent);
          console.log('[Turnstile] Dispatched click to iframe');
        } catch (e) {
          console.log('[Turnstile] Click iframe failed:', e);
        }
      }
    }
  } catch (e) {
    console.log('[Turnstile] Auto-click error:', e);
  }
}

// 清理
onUnmounted(() => {
  cleanupAll();
});
</script>

<style scoped>
.turnstile-container {
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 20px 0;
}

.turnstile-tip {
  margin-bottom: 20px;
  color: #606266;
  font-size: 14px;
}

.turnstile-wrapper {
  min-height: 65px;
  display: flex;
  justify-content: center;
  align-items: center;
}

.status-text {
  margin-top: 15px;
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 14px;
}

.status-text.loading {
  color: #909399;
}

.status-text.success {
  color: #67c23a;
}

.status-text.error {
  color: #f56c6c;
}

.is-loading {
  animation: rotating 1s linear infinite;
}

@keyframes rotating {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}
</style>
