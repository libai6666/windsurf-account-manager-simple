<template>
  <el-dialog
    v-model="dialogVisible"
    title="批量试用链接"
    width="750px"
    :close-on-click-modal="false"
    :close-on-press-escape="false"
    :append-to-body="true"
    :show-close="!isOpening"
    @close="handleClose"
  >
    <div class="batch-links-container">
      <!-- 顶部操作栏 -->
      <div class="links-toolbar">
        <div class="toolbar-left">
          <el-checkbox
            :model-value="isPageAllSelected"
            :indeterminate="isPageIndeterminate"
            @change="handleSelectCurrentPage"
          >
            本页全选
          </el-checkbox>
          <el-checkbox
            :model-value="isAllSelected"
            :indeterminate="isAllIndeterminate"
            @change="handleSelectAll"
            style="margin-left: 12px;"
          >
            全选 ({{ selectedLinks.size }}/{{ successCount }})
          </el-checkbox>
        </div>
        <div class="toolbar-actions">
          <el-select
            v-model="openDelay"
            size="small"
            style="width: 100px;"
            :disabled="isOpening"
          >
            <el-option :value="1" label="间隔 1s" />
            <el-option :value="2" label="间隔 2s" />
            <el-option :value="3" label="间隔 3s" />
            <el-option :value="5" label="间隔 5s" />
            <el-option :value="8" label="间隔 8s" />
            <el-option :value="10" label="间隔 10s" />
          </el-select>
          <el-button
            type="primary"
            size="small"
            :icon="ChromeFilled"
            :disabled="selectedLinks.size === 0 || isOpening"
            :loading="isOpening"
            @click="handleOpenSelected"
          >
            打开选中 ({{ selectedLinks.size }})
          </el-button>
          <el-button
            size="small"
            :icon="CopyDocument"
            :disabled="selectedLinks.size === 0"
            @click="handleCopySelected"
          >
            复制选中
          </el-button>
        </div>
      </div>

      <!-- 链接列表（分页） -->
      <div class="links-list">
        <div
          v-for="item in currentPageItems"
          :key="item.realIndex"
          class="link-item"
          :class="{
            'is-selected': selectedLinks.has(item.realIndex),
            'is-failed': !item.data.success,
            'is-opened': openedLinks.has(item.realIndex)
          }"
        >
          <el-checkbox
            :model-value="selectedLinks.has(item.realIndex)"
            :disabled="!item.data.success"
            @change="(val: boolean) => handleToggleLink(item.realIndex, val)"
          />
          <div class="link-info">
            <div class="link-email">
              <span class="email-text">{{ item.data.email }}</span>
              <el-tag v-if="item.data.success" type="success" size="small">成功</el-tag>
              <el-tag v-else type="danger" size="small">{{ item.data.error || '失败' }}</el-tag>
              <el-tag v-if="openedLinks.has(item.realIndex)" type="warning" size="small" effect="light">已打开</el-tag>
            </div>
            <div v-if="item.data.success && item.data.url" class="link-url">
              <el-link
                type="primary"
                :underline="false"
                class="url-text"
                @click="handleOpenSingle(item.data.url!, item.realIndex)"
              >
                {{ item.data.url }}
              </el-link>
            </div>
          </div>
        </div>
      </div>

      <!-- 分页器 -->
      <div class="links-pagination">
        <el-pagination
          v-model:current-page="currentPage"
          v-model:page-size="pageSize"
          :page-sizes="[10, 20, 50, 100]"
          :total="links.length"
          :small="true"
          layout="total, sizes, prev, pager, next, jumper"
          @size-change="handlePageSizeChange"
          @current-change="handlePageChange"
        />
      </div>

      <!-- 打开进度条 -->
      <div v-if="isOpening" class="links-progress">
        <div class="progress-header">
          <span class="progress-text">正在打开链接 ({{ openProgress.current }}/{{ openProgress.total }})</span>
          <span class="progress-percent">{{ Math.round(openProgress.current / openProgress.total * 100) }}%</span>
        </div>
        <el-progress 
          :percentage="Math.round(openProgress.current / openProgress.total * 100)" 
          :stroke-width="8"
          :show-text="false"
          status="success"
        />
        <div v-if="openProgress.currentEmail" class="progress-current">
          正在打开: {{ openProgress.currentEmail }}
        </div>
      </div>

      <!-- 统计信息 -->
      <div class="links-summary">
        <span class="summary-text">
          共 {{ links.length }} 个账号，成功 {{ successCount }} 个，失败 {{ failedCount }} 个
          <template v-if="openedLinks.size > 0">
            ，已打开 {{ openedLinks.size }} 个
          </template>
        </span>
      </div>
    </div>

    <template #footer>
      <el-button @click="handleClose" :disabled="isOpening">关闭</el-button>
      <el-button
        type="primary"
        :icon="ChromeFilled"
        :disabled="selectedLinks.size === 0 || isOpening"
        :loading="isOpening"
        @click="handleOpenSelected"
      >
        打开选中链接 ({{ selectedLinks.size }})
      </el-button>
    </template>
  </el-dialog>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { ElMessage } from 'element-plus';
import { invoke } from '@tauri-apps/api/core';
import { ChromeFilled, CopyDocument } from '@element-plus/icons-vue';
import { useSettingsStore } from '@/store';

export interface TrialLinkItem {
  email: string;
  success: boolean;
  url?: string;
  error?: string;
}

const props = defineProps<{
  modelValue: boolean;
  links: TrialLinkItem[];
}>();

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void;
}>();

const settingsStore = useSettingsStore();
const dialogVisible = ref(false);
const selectedLinks = ref<Set<number>>(new Set());
const openedLinks = ref<Set<number>>(new Set());
const isOpening = ref(false);
const openProgress = ref({ current: 0, total: 0, currentEmail: '' });
const openDelay = ref(3);

// 分页
const currentPage = ref(1);
const pageSize = ref(20);

// 同步 visible
watch(() => props.modelValue, (val) => {
  dialogVisible.value = val;
  if (val) {
    // 打开时重置状态
    currentPage.value = 1;
    openedLinks.value = new Set();
    // 默认不选中任何链接，让用户手动选择
    selectedLinks.value = new Set();
  }
});

watch(dialogVisible, (val) => {
  emit('update:modelValue', val);
});

const successCount = computed(() => props.links.filter(l => l.success).length);
const failedCount = computed(() => props.links.filter(l => !l.success).length);
const successLinks = computed(() => 
  props.links
    .map((item, index) => ({ item, index }))
    .filter(({ item }) => item.success && item.url)
);

// 当前页数据（带原始索引）
const currentPageItems = computed(() => {
  const start = (currentPage.value - 1) * pageSize.value;
  const end = start + pageSize.value;
  return props.links.slice(start, end).map((data, i) => ({
    data,
    realIndex: start + i
  }));
});

// 当前页中成功的链接索引
const currentPageSuccessIndices = computed(() => 
  currentPageItems.value
    .filter(item => item.data.success && item.data.url)
    .map(item => item.realIndex)
);

// 本页全选状态
const isPageAllSelected = computed(() => {
  if (currentPageSuccessIndices.value.length === 0) return false;
  return currentPageSuccessIndices.value.every(idx => selectedLinks.value.has(idx));
});

const isPageIndeterminate = computed(() => {
  if (currentPageSuccessIndices.value.length === 0) return false;
  const selectedInPage = currentPageSuccessIndices.value.filter(idx => selectedLinks.value.has(idx)).length;
  return selectedInPage > 0 && selectedInPage < currentPageSuccessIndices.value.length;
});

// 全局全选状态
const isAllSelected = computed(() => {
  return successLinks.value.length > 0 && selectedLinks.value.size === successLinks.value.length;
});

const isAllIndeterminate = computed(() => {
  return selectedLinks.value.size > 0 && selectedLinks.value.size < successLinks.value.length;
});

// 本页全选/取消
function handleSelectCurrentPage(val: boolean | string | number) {
  const newSet = new Set(selectedLinks.value);
  if (val) {
    currentPageSuccessIndices.value.forEach(idx => newSet.add(idx));
  } else {
    currentPageSuccessIndices.value.forEach(idx => newSet.delete(idx));
  }
  selectedLinks.value = newSet;
}

// 全局全选/取消
function handleSelectAll(val: boolean | string | number) {
  if (val) {
    selectedLinks.value = new Set(successLinks.value.map(({ index }) => index));
  } else {
    selectedLinks.value = new Set();
  }
}

function handleToggleLink(index: number, val: boolean) {
  const newSet = new Set(selectedLinks.value);
  if (val) {
    newSet.add(index);
  } else {
    newSet.delete(index);
  }
  selectedLinks.value = newSet;
}

function handlePageSizeChange() {
  currentPage.value = 1;
}

function handlePageChange() {
  // 切换页面时保持选中状态不变（跨页选择）
}

async function handleOpenSelected() {
  if (selectedLinks.value.size === 0) return;

  isOpening.value = true;
  const browserMode = settingsStore.settings?.browserMode ?? 'incognito';
  const openCommand = browserMode === 'incognito' ? 'open_external_link_incognito' : 'open_external_link';

  const selectedIndices = Array.from(selectedLinks.value);
  openProgress.value = { current: 0, total: selectedIndices.length, currentEmail: '' };

  let openedCount = 0;
  let openFailedCount = 0;

  try {
    for (let i = 0; i < selectedIndices.length; i++) {
      const idx = selectedIndices[i];
      const item = props.links[idx];
      if (!item?.url) continue;

      openProgress.value = { current: i + 1, total: selectedIndices.length, currentEmail: item.email };

      try {
        await invoke(openCommand, { url: item.url });
        openedCount++;
        // 标记为已打开
        const newOpened = new Set(openedLinks.value);
        newOpened.add(idx);
        openedLinks.value = newOpened;
        // 按用户设置的间隔打开下一个链接
        await new Promise(resolve => setTimeout(resolve, openDelay.value * 1000));
      } catch (err) {
        console.error(`打开链接失败: ${item.email}`, err);
        openFailedCount++;
      }
    }

    if (openFailedCount === 0) {
      ElMessage.success(`已打开 ${openedCount} 个链接`);
    } else {
      ElMessage.warning(`已打开 ${openedCount} 个链接，${openFailedCount} 个打开失败`);
    }
  } finally {
    isOpening.value = false;
    openProgress.value = { current: 0, total: 0, currentEmail: '' };
    // 打开完成后清除选中，方便下一批操作
    selectedLinks.value = new Set();
  }
}

async function handleOpenSingle(url: string, index: number) {
  const browserMode = settingsStore.settings?.browserMode ?? 'incognito';
  const openCommand = browserMode === 'incognito' ? 'open_external_link_incognito' : 'open_external_link';

  try {
    await invoke(openCommand, { url });
    // 标记为已打开
    const newOpened = new Set(openedLinks.value);
    newOpened.add(index);
    openedLinks.value = newOpened;
    ElMessage.success('已在浏览器中打开');
  } catch (err) {
    ElMessage.error('打开链接失败');
  }
}

async function handleCopySelected() {
  const urls = Array.from(selectedLinks.value)
    .map(index => props.links[index])
    .filter(item => item?.url)
    .map(item => item.url)
    .join('\n');

  try {
    await navigator.clipboard.writeText(urls);
    ElMessage.success(`已复制 ${selectedLinks.value.size} 个链接到剪贴板`);
  } catch {
    ElMessage.error('复制失败');
  }
}

function handleClose() {
  if (isOpening.value) return;
  dialogVisible.value = false;
  selectedLinks.value = new Set();
  openedLinks.value = new Set();
}
</script>

<style scoped>
.batch-links-container {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.links-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  background: #f5f7fa;
  border-radius: 6px;
  flex-wrap: wrap;
  gap: 8px;
}

.toolbar-left {
  display: flex;
  align-items: center;
}

.toolbar-actions {
  display: flex;
  gap: 8px;
}

.links-list {
  max-height: 420px;
  overflow-y: auto;
  border: 1px solid #ebeef5;
  border-radius: 6px;
}

.link-item {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 10px 12px;
  border-bottom: 1px solid #f0f0f0;
  transition: background-color 0.2s;
}

.link-item:last-child {
  border-bottom: none;
}

.link-item:hover {
  background-color: #f5f7fa;
}

.link-item.is-selected {
  background-color: #ecf5ff;
}

.link-item.is-failed {
  opacity: 0.7;
}

.link-item.is-opened {
  border-left: 3px solid #e6a23c;
}

.link-info {
  flex: 1;
  min-width: 0;
}

.link-email {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 4px;
  flex-wrap: wrap;
}

.email-text {
  font-weight: 500;
  font-size: 14px;
  color: #303133;
}

.link-url {
  padding-left: 0;
}

.url-text {
  font-size: 12px;
  word-break: break-all;
  max-width: 100%;
}

.links-pagination {
  display: flex;
  justify-content: center;
  padding: 4px 0;
}

.links-progress {
  padding: 10px 12px;
  background: #f0f9eb;
  border: 1px solid #e1f3d8;
  border-radius: 6px;
}

.progress-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 6px;
}

.progress-text {
  font-size: 13px;
  font-weight: 500;
  color: #67c23a;
}

.progress-percent {
  font-size: 13px;
  font-weight: 600;
  color: #67c23a;
}

.progress-current {
  margin-top: 4px;
  font-size: 12px;
  color: #909399;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.links-summary {
  padding: 8px 12px;
  background: #f5f7fa;
  border-radius: 6px;
  text-align: center;
}

.summary-text {
  font-size: 13px;
  color: #909399;
}
</style>
