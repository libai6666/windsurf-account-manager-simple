<template>
  <div class="device-manager-panel">
    <!-- 当前账号信息卡片 -->
    <div class="section-card">
      <div class="section-header">
        <el-icon><Monitor /></el-icon>
        <span>当前 Windsurf 状态</span>
        <el-button size="small" :icon="Refresh" circle @click="refreshAll" :loading="refreshing" style="margin-left: auto;" />
      </div>
      <div class="info-grid" v-if="windsurfInfo.is_active">
        <div class="info-item">
          <span class="info-label">登录邮箱</span>
          <span class="info-value">{{ windsurfInfo.email || '未知' }}</span>
        </div>
        <div class="info-item">
          <span class="info-label">用户名</span>
          <span class="info-value">{{ windsurfInfo.name || '未知' }}</span>
        </div>
        <div class="info-item">
          <span class="info-label">套餐类型</span>
          <el-tag :type="getPlanTagType(windsurfInfo.plan_name)" size="small">
            {{ windsurfInfo.plan_name || 'Free' }}
          </el-tag>
        </div>
        <div class="info-item">
          <span class="info-label">Windsurf 版本</span>
          <span class="info-value">{{ windsurfInfo.version || '未知' }}</span>
        </div>
        <div class="info-item" v-if="matchedAccount">
          <span class="info-label">订阅到期</span>
          <span class="info-value">{{ formatExpiry(matchedAccount.subscription_expires_at) }}</span>
        </div>
        <div class="info-item quota-row" v-if="matchedAccount?.daily_quota_remaining !== undefined || matchedAccount?.weekly_quota_remaining !== undefined">
          <div class="quota-pair">
            <div class="quota-item" v-if="matchedAccount?.daily_quota_remaining !== undefined">
              <span class="info-label">每日配额</span>
              <el-progress
                :percentage="matchedAccount.daily_quota_remaining"
                :stroke-width="14"
                :color="getQuotaColor(matchedAccount.daily_quota_remaining)"
                class="quota-progress"
              />
            </div>
            <div class="quota-item" v-if="matchedAccount?.weekly_quota_remaining !== undefined">
              <span class="info-label">每周配额</span>
              <el-progress
                :percentage="matchedAccount.weekly_quota_remaining"
                :stroke-width="14"
                :color="getQuotaColor(matchedAccount.weekly_quota_remaining)"
                class="quota-progress"
              />
            </div>
          </div>
        </div>
      </div>
      <div v-else class="empty-state">
        <el-empty description="Windsurf 未登录或未运行" :image-size="60" />
      </div>
      <div class="section-actions" style="margin-top: 12px;">
        <el-button type="primary" :icon="Switch" @click="goToSwitchAccount">
          去更换账号
        </el-button>
        <el-select
          v-model="switchTargetGroup"
          placeholder="选择分组"
          size="default"
          style="width: 160px; margin-left: 8px;"
        >
          <el-option
            v-for="group in settingsStore.groups"
            :key="group"
            :label="group"
            :value="group"
          />
        </el-select>
      </div>
    </div>

    <!-- 切号设备码设置 -->
    <div class="section-card">
      <div class="section-header">
        <el-icon><Setting /></el-icon>
        <span>切号设备码设置</span>
      </div>
      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">切号时更换机器设备码</span>
          <span class="setting-desc">开启后，每次切换账号时会自动生成新的机器设备码（防止被限制）。关闭则保持当前设备码不变。</span>
        </div>
        <el-switch
          v-model="resetMachineIdOnSwitch"
          @change="handleResetSettingChange"
          active-text="开启"
          inactive-text="关闭"
        />
      </div>
    </div>

    <!-- 当前设备码信息 -->
    <div class="section-card">
      <div class="section-header">
        <el-icon><Key /></el-icon>
        <span>当前机器设备码</span>
        <div style="margin-left: auto; display: flex; gap: 8px;">
          <el-button size="small" type="warning" :loading="resetting" @click="handleResetMachineId">
            <el-icon><RefreshRight /></el-icon>
            重置机器码
          </el-button>
          <el-button size="small" type="primary" @click="showSaveDialog">
            <el-icon><Plus /></el-icon>
            保存当前设备码
          </el-button>
        </div>
      </div>
      <div class="machine-id-display" v-if="currentMachineIds.machine_id">
        <div class="id-row">
          <span class="id-label">machineId</span>
          <el-tooltip :content="currentMachineIds.machine_id" placement="top">
            <span class="id-value monospace">{{ truncateId(currentMachineIds.machine_id) }}</span>
          </el-tooltip>
          <el-button :icon="CopyDocument" size="small" link @click="copyToClipboard(currentMachineIds.machine_id!)" />
        </div>
        <div class="id-row">
          <span class="id-label">macMachineId</span>
          <el-tooltip :content="currentMachineIds.mac_machine_id" placement="top">
            <span class="id-value monospace">{{ truncateId(currentMachineIds.mac_machine_id) }}</span>
          </el-tooltip>
          <el-button :icon="CopyDocument" size="small" link @click="copyToClipboard(currentMachineIds.mac_machine_id!)" />
        </div>
        <div class="id-row">
          <span class="id-label">sqmId</span>
          <el-tooltip :content="currentMachineIds.sqm_id" placement="top">
            <span class="id-value monospace">{{ truncateId(currentMachineIds.sqm_id) }}</span>
          </el-tooltip>
          <el-button :icon="CopyDocument" size="small" link @click="copyToClipboard(currentMachineIds.sqm_id!)" />
        </div>
        <div class="id-row">
          <span class="id-label">devDeviceId</span>
          <el-tooltip :content="currentMachineIds.dev_device_id" placement="top">
            <span class="id-value monospace">{{ truncateId(currentMachineIds.dev_device_id) }}</span>
          </el-tooltip>
          <el-button :icon="CopyDocument" size="small" link @click="copyToClipboard(currentMachineIds.dev_device_id!)" />
        </div>
        <div class="id-row" v-if="currentMachineIds.registry_machine_guid">
          <span class="id-label">MachineGuid</span>
          <el-tooltip :content="currentMachineIds.registry_machine_guid" placement="top">
            <span class="id-value monospace">{{ truncateId(currentMachineIds.registry_machine_guid) }}</span>
          </el-tooltip>
          <el-button :icon="CopyDocument" size="small" link @click="copyToClipboard(currentMachineIds.registry_machine_guid!)" />
        </div>
      </div>
      <div v-else class="empty-state">
        <el-empty description="无法读取当前设备码" :image-size="40" />
      </div>
    </div>

    <!-- 收藏设备码快捷列表 -->
    <div class="section-card" v-if="bookmarkedRecords.length > 0">
      <div class="section-header">
        <el-icon><Star /></el-icon>
        <span>收藏设备码</span>
        <span class="record-count">({{ bookmarkedRecords.length }} 个)</span>
      </div>
      <div class="bookmarked-list">
        <div
          v-for="record in paginatedBookmarked"
          :key="'bm-' + record.id"
          class="bookmarked-item"
          :class="{ 'is-current': record.is_current }"
        >
          <div class="bookmarked-info">
            <el-icon class="bookmark-icon"><StarFilled /></el-icon>
            <span class="bookmarked-label">{{ record.label }}</span>
            <el-tag v-if="record.is_current" type="success" size="small" effect="dark">当前</el-tag>
            <el-tag v-if="record.last_associated_email" type="info" size="small" effect="plain">
              {{ record.last_associated_email }}
            </el-tag>
          </div>
          <div class="record-actions">
            <el-tooltip content="切换到此设备码" placement="top">
              <el-button
                type="primary"
                size="small"
                :icon="Switch"
                :disabled="record.is_current"
                :loading="applyingId === record.id"
                @click="handleApplyMachineId(record)"
              >
                切换
              </el-button>
            </el-tooltip>
            <el-tooltip content="编辑标签" placement="top">
              <el-button size="small" :icon="Edit" @click="showEditDialog(record)" />
            </el-tooltip>
            <el-tooltip content="取消收藏" placement="top">
              <el-button size="small" :icon="Star" @click="handleToggleBookmark(record, false)" />
            </el-tooltip>
          </div>
        </div>
        <!-- 收藏分页 -->
        <div class="pagination-wrapper" v-if="bookmarkedRecords.length > bookmarkPageSize">
          <el-pagination
            v-model:current-page="bookmarkPage"
            :page-size="bookmarkPageSize"
            :total="bookmarkedRecords.length"
            layout="prev, pager, next"
            small
            background
          />
        </div>
      </div>
    </div>

    <!-- 设备码历史列表 -->
    <div class="section-card">
      <div class="section-header">
        <el-icon><List /></el-icon>
        <span>设备码历史记录</span>
        <span class="record-count" v-if="machineIdRecords.length > 0">
          ({{ machineIdRecords.length }} 条)
        </span>
        <el-button
          v-if="machineIdRecords.length > 0"
          size="small"
          type="danger"
          plain
          :icon="Delete"
          style="margin-left: auto;"
          @click="handleClearAllRecords"
        >
          清空全部
        </el-button>
      </div>
      <div v-if="machineIdRecords.length === 0" class="empty-state">
        <el-empty description="暂无设备码记录，切号时会自动保存" :image-size="40" />
      </div>
      <div v-else class="records-list">
        <div
          v-for="record in paginatedRecords"
          :key="record.id"
          class="record-item"
          :class="{ 'is-current': record.is_current }"
        >
          <div class="record-main">
            <div class="record-top">
              <el-tag v-if="record.is_current" type="success" size="small" effect="dark">当前</el-tag>
              <span class="record-label">{{ record.label }}</span>
              <el-tag v-if="record.last_associated_email" type="info" size="small" effect="plain">
                {{ record.last_associated_email }}
              </el-tag>
            </div>
            <div class="record-ids">
              <el-tooltip :content="record.machine_id" placement="top">
                <span class="record-id-preview monospace">machineId: {{ truncateId(record.machine_id, 16) }}</span>
              </el-tooltip>
            </div>
            <div class="record-meta">
              <span v-if="record.note" class="record-note">{{ record.note }}</span>
              <span class="record-time">{{ formatTime(record.created_at) }}</span>
              <span v-if="record.last_used_at" class="record-time">
                最后使用: {{ formatTime(record.last_used_at) }}
              </span>
            </div>
          </div>
          <div class="record-actions">
            <el-tooltip content="切换到此设备码" placement="top">
              <el-button
                type="primary"
                size="small"
                :icon="Switch"
                :disabled="record.is_current"
                :loading="applyingId === record.id"
                @click="handleApplyMachineId(record)"
              >
                切换
              </el-button>
            </el-tooltip>
            <el-tooltip :content="record.is_bookmarked ? '取消收藏' : '收藏'" placement="top">
              <el-button
                size="small"
                :icon="record.is_bookmarked ? StarFilled : Star"
                :type="record.is_bookmarked ? 'warning' : 'default'"
                @click="handleToggleBookmark(record, !record.is_bookmarked)"
              />
            </el-tooltip>
            <el-tooltip content="编辑标签" placement="top">
              <el-button
                size="small"
                :icon="Edit"
                @click="showEditDialog(record)"
              />
            </el-tooltip>
            <el-tooltip content="删除" placement="top">
              <el-button
                type="danger"
                size="small"
                :icon="Delete"
                @click="handleDeleteRecord(record)"
              />
            </el-tooltip>
          </div>
        </div>
        <!-- 分页 -->
        <div class="pagination-wrapper" v-if="machineIdRecords.length > pageSize">
          <el-pagination
            v-model:current-page="currentPage"
            :page-size="pageSize"
            :total="machineIdRecords.length"
            layout="prev, pager, next"
            small
            background
          />
        </div>
      </div>
    </div>

    <!-- 保存设备码对话框 -->
    <el-dialog v-model="saveDialogVisible" title="保存当前设备码" width="450px" :close-on-click-modal="false">
      <el-form :model="saveForm" label-width="80px">
        <el-form-item label="标签" required>
          <el-input v-model="saveForm.label" placeholder="如：主力机、备用机" maxlength="30" show-word-limit />
        </el-form-item>
        <el-form-item label="备注">
          <el-input v-model="saveForm.note" type="textarea" :rows="2" placeholder="可选备注信息" maxlength="100" show-word-limit />
        </el-form-item>
        <el-form-item label="收藏">
          <el-switch v-model="saveForm.bookmarked" active-text="是" inactive-text="否" />
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="saveDialogVisible = false">取消</el-button>
        <el-button type="primary" :loading="saving" :disabled="!saveForm.label.trim()" @click="handleSaveCurrentMachineId">
          保存
        </el-button>
      </template>
    </el-dialog>

    <!-- 编辑标签对话框 -->
    <el-dialog v-model="editDialogVisible" title="编辑设备码标签" width="450px" :close-on-click-modal="false">
      <el-form :model="editForm" label-width="80px">
        <el-form-item label="标签" required>
          <el-input v-model="editForm.label" placeholder="设备码标签" maxlength="30" show-word-limit />
        </el-form-item>
        <el-form-item label="备注">
          <el-input v-model="editForm.note" type="textarea" :rows="2" placeholder="可选备注信息" maxlength="100" show-word-limit />
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="editDialogVisible = false">取消</el-button>
        <el-button type="primary" :disabled="!editForm.label.trim()" @click="handleUpdateLabel">
          保存
        </el-button>
      </template>
    </el-dialog>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, h } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import {
  Monitor,
  Setting,
  Key,
  List,
  Switch,
  Plus,
  Edit,
  Delete,
  Refresh,
  RefreshRight,
  CopyDocument,
  Star,
  StarFilled,
} from '@element-plus/icons-vue';
import { machineIdApi, settingsApi, apiService } from '@/api';
import { useSettingsStore, useAccountsStore } from '@/store';
import type { MachineIdRecord, CurrentMachineIds } from '@/types';
import dayjs from 'dayjs';

const settingsStore = useSettingsStore();
const accountsStore = useAccountsStore();

const emit = defineEmits<{
  (e: 'switch-to-group', group: string): void;
}>();

// 状态
const refreshing = ref(false);
const saving = ref(false);
const resetting = ref(false);
const applyingId = ref<string | null>(null);
const saveDialogVisible = ref(false);
const editDialogVisible = ref(false);
const switchTargetGroup = ref('默认分组');
const resetMachineIdOnSwitch = ref(true);

// 数据
const windsurfInfo = ref<{
  email?: string;
  name?: string;
  api_key?: string;
  plan_name?: string;
  team_id?: string;
  version?: string;
  is_active: boolean;
  remaining_usage?: number | null;
  total_usage?: number | null;
}>({ is_active: false });

const currentMachineIds = ref<CurrentMachineIds>({});
const machineIdRecords = ref<MachineIdRecord[]>([]);

// 历史列表分页
const currentPage = ref(1);
const pageSize = 5;
const paginatedRecords = computed(() => {
  const start = (currentPage.value - 1) * pageSize;
  return machineIdRecords.value.slice(start, start + pageSize);
});

// 收藏列表分页
const bookmarkPage = ref(1);
const bookmarkPageSize = 5;
const bookmarkedRecords = computed(() => {
  return machineIdRecords.value.filter(r => r.is_bookmarked);
});
const paginatedBookmarked = computed(() => {
  const start = (bookmarkPage.value - 1) * bookmarkPageSize;
  return bookmarkedRecords.value.slice(start, start + bookmarkPageSize);
});

// 表单
const saveForm = ref({ label: '', note: '', bookmarked: false });
const editForm = ref({ id: '', label: '', note: '' });
const clearKeepBookmarked = ref(true);

// 匹配当前登录邮箱对应的账号
const matchedAccount = computed(() => {
  if (!windsurfInfo.value.email) return null;
  return accountsStore.accounts.find(a => a.email === windsurfInfo.value.email);
});

// 初始化
onMounted(async () => {
  await refreshAll();
  // 同步设置
  if (settingsStore.settings) {
    resetMachineIdOnSwitch.value = settingsStore.settings.resetMachineIdOnSwitch !== false;
    switchTargetGroup.value = settingsStore.settings.autoSwitchGroup || '默认分组';
  }
});

// 刷新所有数据
async function refreshAll() {
  refreshing.value = true;
  try {
    await Promise.all([
      loadWindsurfInfo(),
      loadCurrentMachineIds(),
      loadMachineIdRecords(),
    ]);
  } finally {
    refreshing.value = false;
  }
}

async function loadWindsurfInfo() {
  try {
    windsurfInfo.value = await settingsApi.getCurrentWindsurfInfo();
  } catch (e) {
    console.error('Failed to load windsurf info:', e);
  }
}

async function loadCurrentMachineIds() {
  try {
    currentMachineIds.value = await machineIdApi.getCurrentMachineIds();
  } catch (e) {
    console.error('Failed to load current machine IDs:', e);
  }
}

async function loadMachineIdRecords() {
  try {
    const records: MachineIdRecord[] = await machineIdApi.getMachineIdRecords();
    // 按时间倒序排列，当前的排最前
    machineIdRecords.value = records.sort((a: MachineIdRecord, b: MachineIdRecord) => {
      if (a.is_current && !b.is_current) return -1;
      if (!a.is_current && b.is_current) return 1;
      return new Date(b.created_at).getTime() - new Date(a.created_at).getTime();
    });
  } catch (e) {
    console.error('Failed to load machine ID records:', e);
  }
}

// 切号设备码设置变更
async function handleResetSettingChange(val: boolean) {
  try {
    const currentSettings = await settingsApi.getSettings();
    currentSettings.resetMachineIdOnSwitch = val;
    await settingsApi.updateSettings(currentSettings);
    // 同步 store
    if (settingsStore.settings) {
      settingsStore.settings.resetMachineIdOnSwitch = val;
    }
    ElMessage.success(val ? '已开启切号时更换设备码' : '已关闭切号时更换设备码');
  } catch (e) {
    ElMessage.error('设置保存失败');
    resetMachineIdOnSwitch.value = !val;
  }
}

// 重置机器码（生成全新设备码）
async function handleResetMachineId() {
  try {
    await ElMessageBox.confirm(
      '确定要重置机器码吗？将生成全新的设备码。重置前会自动保存当前设备码到历史记录。',
      '重置机器码',
      { type: 'warning', confirmButtonText: '确认重置', cancelButtonText: '取消' }
    );
  } catch {
    return;
  }

  resetting.value = true;
  try {
    // 先保存当前设备码
    await machineIdApi.saveCurrentMachineId(
      `重置前自动保存 ${dayjs().format('MM-DD HH:mm')}`,
      '重置机器码前自动保存',
      windsurfInfo.value.email || undefined,
    );
    // 调用重置
    const result = await apiService.resetMachineId();
    if (result.success) {
      ElMessage.success(result.message || '机器码已重置');
      await Promise.all([loadCurrentMachineIds(), loadMachineIdRecords()]);
    } else {
      ElMessage.error(result.error || '重置失败，可能需要管理员权限');
    }
  } catch (e) {
    ElMessage.error('重置机器码失败');
  } finally {
    resetting.value = false;
  }
}

// 保存当前设备码
function showSaveDialog() {
  saveForm.value = {
    label: '',
    note: '',
    bookmarked: false,
  };
  saveDialogVisible.value = true;
}

async function handleSaveCurrentMachineId() {
  if (!saveForm.value.label.trim()) return;
  saving.value = true;
  try {
    const result = await machineIdApi.saveCurrentMachineId(
      saveForm.value.label.trim(),
      saveForm.value.note.trim() || undefined,
      windsurfInfo.value.email || undefined,
      undefined,
      saveForm.value.bookmarked,
    );
    if (result.success) {
      ElMessage.success('设备码已保存');
      saveDialogVisible.value = false;
      await loadMachineIdRecords();
    } else {
      ElMessage.error(result.error || '保存失败');
    }
  } catch (e) {
    ElMessage.error('保存设备码失败');
  } finally {
    saving.value = false;
  }
}

// 应用设备码
async function handleApplyMachineId(record: MachineIdRecord) {
  try {
    await ElMessageBox.confirm(
      `确定要切换到设备码 "${record.label}" 吗？\n这将替换当前系统的所有机器标识。`,
      '切换设备码',
      { type: 'warning', confirmButtonText: '确认切换', cancelButtonText: '取消' }
    );
  } catch {
    return;
  }

  applyingId.value = record.id;
  try {
    const result = await machineIdApi.applyMachineId(record.id);
    if (result.success) {
      ElMessage.success(result.message || '设备码已切换');
      await Promise.all([loadCurrentMachineIds(), loadMachineIdRecords()]);
    } else {
      ElMessage.error(result.error || '切换失败');
    }
  } catch (e) {
    ElMessage.error('切换设备码失败');
  } finally {
    applyingId.value = null;
  }
}

// 编辑标签
function showEditDialog(record: MachineIdRecord) {
  editForm.value = {
    id: record.id,
    label: record.label,
    note: record.note || '',
  };
  editDialogVisible.value = true;
}

async function handleUpdateLabel() {
  if (!editForm.value.label.trim()) return;
  try {
    const result = await machineIdApi.updateMachineIdLabel(
      editForm.value.id,
      editForm.value.label.trim(),
      editForm.value.note.trim() || undefined,
    );
    if (result.success) {
      ElMessage.success('标签已更新');
      editDialogVisible.value = false;
      await loadMachineIdRecords();
    }
  } catch (e) {
    ElMessage.error('更新标签失败');
  }
}

// 删除记录
async function handleDeleteRecord(record: MachineIdRecord) {
  try {
    await ElMessageBox.confirm(
      `确定要删除设备码记录 "${record.label}" 吗？删除后无法恢复。`,
      '删除确认',
      { type: 'warning', confirmButtonText: '删除', cancelButtonText: '取消' }
    );
  } catch {
    return;
  }

  try {
    const result = await machineIdApi.deleteMachineIdRecord(record.id);
    if (result.success) {
      ElMessage.success('已删除');
      await loadMachineIdRecords();
    }
  } catch (e) {
    ElMessage.error('删除失败');
  }
}

// 收藏/取消收藏
async function handleToggleBookmark(record: MachineIdRecord, bookmarked: boolean) {
  try {
    const result = await machineIdApi.toggleMachineIdBookmark(record.id, bookmarked);
    if (result.success) {
      record.is_bookmarked = bookmarked;
      ElMessage.success(bookmarked ? '已收藏' : '已取消收藏');
    }
  } catch (e) {
    ElMessage.error('操作失败');
  }
}

// 清空设备码记录
async function handleClearAllRecords() {
  clearKeepBookmarked.value = true;
  const bookmarkedCount = bookmarkedRecords.value.length;
  const totalCount = machineIdRecords.value.length;
  
  const msgNodes = [
    h('p', {}, `确定要清空 ${totalCount} 条设备码记录吗？清空后无法恢复。`),
  ];
  if (bookmarkedCount > 0) {
    msgNodes.push(
      h('div', { style: 'margin-top: 12px;' }, [
        h('label', { style: 'display: flex; align-items: center; gap: 8px; cursor: pointer;' }, [
          h('input', {
            type: 'checkbox',
            checked: clearKeepBookmarked.value,
            onChange: (e: Event) => {
              clearKeepBookmarked.value = (e.target as HTMLInputElement).checked;
            },
          }),
          h('span', {}, `保留收藏的设备码 (${bookmarkedCount} 个)`),
        ]),
      ])
    );
  }
  
  try {
    await ElMessageBox({
      title: '清空确认',
      message: h('div', {}, msgNodes),
      showCancelButton: true,
      confirmButtonText: '确认清空',
      cancelButtonText: '取消',
      type: 'warning',
    });
  } catch {
    return;
  }

  try {
    const result = await machineIdApi.clearAllMachineIdRecords(clearKeepBookmarked.value);
    if (result.success) {
      ElMessage.success(result.message || '已清空');
      await loadMachineIdRecords();
      currentPage.value = 1;
      bookmarkPage.value = 1;
    }
  } catch (e) {
    ElMessage.error('清空失败');
  }
}

// 去更换账号 - 跳转到分组
function goToSwitchAccount() {
  emit('switch-to-group', switchTargetGroup.value);
}

// 工具函数
function truncateId(id?: string, len: number = 20): string {
  if (!id) return '-';
  if (id.length <= len) return id;
  return id.substring(0, len) + '...';
}

function formatTime(time?: string): string {
  if (!time) return '-';
  return dayjs(time).format('YYYY-MM-DD HH:mm');
}

function formatExpiry(time?: string): string {
  if (!time) return '未知';
  const d = dayjs(time);
  const diff = d.diff(dayjs(), 'day');
  if (diff < 0) return `已过期 ${Math.abs(diff)} 天`;
  return `${d.format('YYYY-MM-DD')} (剩余 ${diff} 天)`;
}

function getPlanTagType(plan?: string): 'success' | 'warning' | 'danger' | 'info' | '' {
  if (!plan) return 'info';
  const lower = plan.toLowerCase();
  if (lower.includes('pro')) return 'success';
  if (lower.includes('team')) return 'warning';
  if (lower.includes('enterprise')) return '';
  if (lower.includes('free')) return 'info';
  return 'info';
}

function getQuotaColor(pct?: number): string {
  if (pct === undefined) return '#909399';
  if (pct > 50) return '#67c23a';
  if (pct > 20) return '#e6a23c';
  return '#f56c6c';
}

async function copyToClipboard(text: string) {
  try {
    await navigator.clipboard.writeText(text);
    ElMessage.success('已复制到剪贴板');
  } catch {
    ElMessage.error('复制失败');
  }
}
</script>

<style scoped>
.device-manager-panel {
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 16px;
  overflow-y: auto;
  height: 100%;
}

.section-card {
  background: var(--el-bg-color);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
  padding: 16px;
}

.section-header {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 15px;
  font-weight: 600;
  margin-bottom: 14px;
  color: var(--el-text-color-primary);
}

.record-count {
  font-size: 13px;
  color: var(--el-text-color-secondary);
  font-weight: normal;
}

.info-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 12px 24px;
}

.info-item {
  display: flex;
  align-items: center;
  gap: 8px;
}

.info-label {
  color: var(--el-text-color-secondary);
  font-size: 13px;
  min-width: 70px;
  flex-shrink: 0;
}

.info-value {
  font-size: 13px;
  color: var(--el-text-color-primary);
  word-break: break-all;
}

.section-actions {
  display: flex;
  align-items: center;
}

.setting-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}

.setting-info {
  flex: 1;
}

.setting-label {
  font-size: 14px;
  font-weight: 500;
  color: var(--el-text-color-primary);
  display: block;
}

.setting-desc {
  font-size: 12px;
  color: var(--el-text-color-secondary);
  margin-top: 4px;
  display: block;
}

.machine-id-display {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.id-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.id-label {
  font-size: 13px;
  color: var(--el-text-color-secondary);
  min-width: 100px;
  flex-shrink: 0;
}

.id-value {
  font-size: 13px;
  color: var(--el-text-color-primary);
  cursor: default;
}

.monospace {
  font-family: 'Cascadia Code', 'Fira Code', 'Consolas', monospace;
}

.empty-state {
  padding: 12px 0;
}

/* 设备码历史列表 */
.records-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.record-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 6px;
  transition: all 0.2s;
}

.record-item:hover {
  border-color: var(--el-color-primary-light-5);
  background: var(--el-fill-color-light);
}

.record-item.is-current {
  border-color: var(--el-color-success-light-3);
  background: var(--el-color-success-light-9);
}

.record-main {
  flex: 1;
  min-width: 0;
}

.record-top {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 4px;
}

.record-label {
  font-size: 14px;
  font-weight: 500;
  color: var(--el-text-color-primary);
}

.record-ids {
  margin-bottom: 4px;
}

.record-id-preview {
  font-size: 12px;
  color: var(--el-text-color-secondary);
}

.record-meta {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}

.record-note {
  font-size: 12px;
  color: var(--el-text-color-regular);
}

.record-time {
  font-size: 12px;
  color: var(--el-text-color-placeholder);
}

.record-actions {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
  margin-left: 12px;
}

/* 收藏设备码快捷列表 */
.bookmarked-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.bookmarked-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 6px;
  transition: all 0.2s;
}

.bookmarked-item:hover {
  border-color: var(--el-color-primary-light-5);
  background: var(--el-fill-color-light);
}

.bookmarked-item.is-current {
  border-color: var(--el-color-success-light-3);
  background: var(--el-color-success-light-9);
}

.bookmarked-info {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.bookmark-icon {
  color: var(--el-color-warning);
  font-size: 14px;
  flex-shrink: 0;
}

.bookmarked-label {
  font-size: 13px;
  font-weight: 500;
  color: var(--el-text-color-primary);
}

.pagination-wrapper {
  display: flex;
  justify-content: center;
  padding-top: 12px;
}

.quota-row {
  grid-column: 1 / -1;
}

.quota-pair {
  display: flex;
  gap: 32px;
}

.quota-item {
  display: flex;
  align-items: center;
  gap: 8px;
}

.quota-progress {
  width: 160px;
}

.quota-progress :deep(.el-progress__text) {
  font-size: 13px !important;
}
</style>
