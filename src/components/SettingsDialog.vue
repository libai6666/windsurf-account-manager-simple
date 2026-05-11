<template>
  <el-dialog
    v-model="uiStore.showSettingsDialog"
    title="设置"
    width="700px"
  >
    <el-tabs v-model="activeTab" type="border-card">
      <!-- 基础设置标签页 -->
      <el-tab-pane label="基础设置" name="basic">
        <el-form :model="settings" label-width="140px">
          <el-form-item label="自动刷新Token">
            <el-switch v-model="settings.auto_refresh_token" />
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              开启后，Token过期时将自动刷新
            </div>
          </el-form-item>
          
          <el-form-item label="全量并发刷新" v-if="settings.auto_refresh_token">
            <el-switch v-model="settings.unlimitedConcurrentRefresh" />
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              开启后，自动刷新Token时所有账号同时并发，不受并发限制，可大幅节省时间
            </div>
          </el-form-item>
          
          <!-- 座位数选项 - simple 版本已禁用
          <el-form-item label="座位数选项">
            <el-input
              v-model="seatCountOptionsInput"
              placeholder="例如: 18, 19, 20"
              style="width: 200px;"
              @blur="parseSeatCountOptions"
            />
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              积分重置时轮番使用的座位数，用逗号分隔（如：18, 19, 20）
            </div>
          </el-form-item>
          -->
          
          <el-form-item label="重试次数">
            <el-input-number
              v-model="settings.retry_times"
              :min="1"
              :max="5"
              :step="1"
            />
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              API调用失败时的重试次数
            </div>
          </el-form-item>
          
          <el-form-item label="并发限制">
            <el-input-number
              v-model="settings.concurrent_limit"
              :min="1"
              :max="30"
              :step="1"
              :disabled="settings.unlimitedConcurrentRefresh"
            />
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              {{ settings.unlimitedConcurrentRefresh ? '已开启全量并发刷新，此设置不影响自动刷新' : '批量操作时的最大并发数，最多 30 个' }}
            </div>
          </el-form-item>
          
          <el-form-item label="界面主题">
            <el-radio-group v-model="settings.theme">
              <el-radio-button label="light">浅色</el-radio-button>
              <el-radio-button label="dark">深色</el-radio-button>
            </el-radio-group>
          </el-form-item>
          
          <el-form-item label="显示详细结果">
            <el-switch 
              v-model="settings.show_seats_result_dialog"
              active-text="开启"
              inactive-text="关闭"
            />
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              开启后，积分重置时将显示详细的座位更新结果对话框
            </div>
          </el-form-item>
          
          <el-form-item label="隐私模式">
            <el-switch 
              v-model="settings.privacyMode"
              active-text="开启"
              inactive-text="关闭"
            />
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              开启后，所有邮箱地址将显示为随机字符，保护隐私（适用于截图演示）
            </div>
          </el-form-item>
          
          <el-divider content-position="left">网络维护</el-divider>
          
          <el-form-item label="轻量级API">
            <el-switch 
              v-model="settings.useLightweightApi"
              active-text="开启"
              inactive-text="关闭"
            />
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              开启时使用 GetPlanStatus 获取配额信息（更快），关闭时使用 GetCurrentUser（数据更完整）
            </div>
          </el-form-item>
          
          <el-form-item label="启用代理">
            <el-switch 
              v-model="settings.proxyEnabled"
              active-text="开启"
              inactive-text="关闭"
            />
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              开启后，登录和刷新Token等 Google API 请求将通过代理进行
            </div>
          </el-form-item>
          
          <el-form-item label="代理地址" v-if="settings.proxyEnabled">
            <el-input
              v-model="settings.proxyUrl"
              placeholder="http://127.0.0.1:7890"
              style="width: 280px;"
              clearable
            >
              <template #prefix>
                <el-icon><Connection /></el-icon>
              </template>
            </el-input>
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              支持 HTTP/HTTPS/SOCKS5 代理，格式：http://host:port 或 socks5://host:port
            </div>
          </el-form-item>
          
          <el-form-item label="重置网络连接">
            <el-button 
              type="warning" 
              @click="handleResetHttpClient"
              :loading="resettingHttp"
            >
              重置HTTP客户端
            </el-button>
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              当遇到连续的API请求失败时，可点击此按钮重置网络连接池
            </div>
          </el-form-item>
        </el-form>
      </el-tab-pane>
      
      <!-- 支付设置标签页 -->
      <el-tab-pane label="支付设置" name="payment">
        <el-form :model="settings" label-width="140px">
          <el-form-item label="自动打开支付页面">
            <el-switch 
              v-model="settings.autoOpenPaymentLinkInWebview"
              active-text="开启"
              inactive-text="关闭"
            />
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              开启后，获取绑卡链接成功时将自动在内置浏览器窗口中打开支付页面（隐私模式，不保存任何数据）
            </div>
          </el-form-item>
          
          <el-divider content-position="left">外部浏览器设置</el-divider>
          
          <el-form-item label="自动打开外部浏览器">
            <el-switch 
              v-model="settings.autoOpenBrowser"
              active-text="开启"
              inactive-text="关闭"
            />
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              开启后，获取绑卡链接时将自动在外部浏览器中打开（无需点击确认）
            </div>
          </el-form-item>
          
          <el-form-item label="浏览器模式">
            <el-radio-group v-model="settings.browserMode">
              <el-radio-button label="incognito">无痕模式</el-radio-button>
              <el-radio-button label="normal">普通模式</el-radio-button>
            </el-radio-group>
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              选择打开外部浏览器时使用的模式（无痕模式更安全，推荐使用）
            </div>
          </el-form-item>
          
          <el-form-item label="自定义浏览器">
            <div style="display: flex; align-items: center; gap: 8px; width: 100%;">
              <el-select
                v-model="selectedBrowserKey"
                placeholder="默认浏览器（Chrome）"
                style="flex: 1;"
                clearable
                @change="handleBrowserSelect"
                @clear="handleBrowserClear"
              >
                <el-option
                  v-for="browser in detectedBrowsers"
                  :key="browser.path"
                  :label="browser.name"
                  :value="browser.path"
                >
                  <span>{{ browser.name }}</span>
                  <span style="color: #909399; font-size: 11px; margin-left: 6px;">{{ browser.path }}</span>
                </el-option>
                <el-option key="__custom__" label="手动指定路径..." value="__custom__">
                  <span style="color: #409EFF;">手动指定路径...</span>
                </el-option>
              </el-select>
              <el-button @click="loadInstalledBrowsers" :loading="detectingBrowsers" :icon="RefreshRight" title="重新检测" />
            </div>
            <el-input
              v-if="showCustomBrowserInput"
              v-model="settings.customBrowserPath"
              placeholder="请输入浏览器可执行文件完整路径"
              clearable
              style="margin-top: 8px;"
            >
              <template #append>
                <el-button @click="browseBrowserPath">
                  浏览
                </el-button>
              </template>
            </el-input>
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              选择系统已安装的浏览器，或手动指定路径。留空则默认使用 Chrome
            </div>
          </el-form-item>
          
          <el-divider content-position="left">自动填写设置</el-divider>
          
          <el-form-item label="自动填写支付表单">
            <el-switch 
              v-model="settings.autoFillPaymentForm"
              active-text="开启"
              inactive-text="关闭"
            />
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              开启后，将自动使用虚拟卡信息填写Stripe支付表单（仅用于测试）
            </div>
          </el-form-item>
          
          <el-form-item label="显示虚拟卡信息">
            <el-switch 
              v-model="settings.showVirtualCardInfo"
              active-text="开启"
              inactive-text="关闭"
            />
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              开启后，自动填写表单时会弹窗显示生成的虚拟卡信息
            </div>
          </el-form-item>
          
          <el-form-item label="自动提交表单">
            <el-switch 
              v-model="settings.autoSubmitPaymentForm"
              active-text="开启"
              inactive-text="关闭"
              :disabled="!settings.autoFillPaymentForm"
            />
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              开启后，表单填写完成后将自动提交（谨慎使用）
            </div>
          </el-form-item>
          
          <el-form-item label="支付页面延迟(秒)">
            <el-input-number
              v-model="settings.paymentPageDelay"
              :min="1"
              :max="10"
              :step="1"
              :disabled="!settings.autoFillPaymentForm"
            />
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              等待多少秒后开始自动填写表单
            </div>
          </el-form-item>
          
          <el-form-item label="自定义卡头">
            <el-input
              v-model="settings.customCardBin"
              placeholder="请输入4-12位数字"
              maxlength="12"
              @input="validateCardBin"
            >
              <template #append>
                <el-button @click="resetCardBin">恢复默认</el-button>
              </template>
            </el-input>
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              设置虚拟卡的前缀（BIN），必须是4-12位数字，默认为626202
            </div>
          </el-form-item>
          
          <el-form-item label="卡段范围（可选）">
            <el-input
              v-model="settings.customCardBinRange"
              placeholder="如：626200-626300"
              @input="validateCardBinRange"
            >
              <template #append>
                <el-button @click="clearCardBinRange">清除</el-button>
              </template>
            </el-input>
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              设置卡段范围后，绑卡时将从范围内随机选择一个BIN。格式：起始BIN-结束BIN
            </div>
          </el-form-item>
          
          <el-form-item label="绑卡失败重试次数">
            <el-input-number
              v-model="settings.cardBindRetryTimes"
              :min="0"
              :max="20"
              :step="1"
              controls-position="right"
            />
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              绑卡失败后自动重新生成卡号重试的次数，设为0则不重试
            </div>
          </el-form-item>
          
          <el-divider content-position="left">卡BIN池功能</el-divider>
          
          <el-form-item label="测试模式">
            <div style="display: flex; align-items: center; gap: 10px;">
              <el-switch v-model="settings.testModeEnabled" />
              <el-button 
                size="small" 
                type="warning" 
                @click="resetTestModeProgress"
                :disabled="!testModeProgress"
              >
                重置进度
              </el-button>
            </div>
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              开启后，按顺序遍历卡BIN范围，并收集成功的BIN（池数量：{{ successBinCount }}）
              <span v-if="testModeProgress" style="color: #67C23A;">
                <br/>当前进度：{{ testModeProgress }}
              </span>
            </div>
          </el-form-item>
          
          <el-form-item label="使用本地BIN池">
            <el-switch v-model="settings.useLocalSuccessBins" :disabled="successBinCount === 0" />
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              开启后，自动从本地成功BIN池中随机获取卡BIN生成卡号
            </div>
          </el-form-item>
          
          <el-form-item label="BIN池管理">
            <el-button-group>
              <el-button size="small" @click="viewSuccessBins" :disabled="successBinCount === 0">
                查看BIN池
              </el-button>
              <el-button size="small" type="danger" @click="clearSuccessBins" :disabled="successBinCount === 0">
                清空BIN池
              </el-button>
            </el-button-group>
          </el-form-item>
          
          <el-alert
            title="重要提示"
            type="warning"
            :closable="false"
            show-icon
            style="margin-top: 20px;"
          >
            <template #default>
              <div style="font-size: 12px; line-height: 1.6;">
                <p>🔒 内置浏览器使用隐私模式，不会保存任何浏览数据、Cookies或历史记录。</p>
                <p>⚠️ 虚拟卡信息生成功能仅用于测试目的，请勿用于实际支付。</p>
                <p>⚠️ 使用本功能时，请确保遵守Stripe及相关支付服务的使用条款。</p>
                <p>⚠️ 不要将生成的虚拟卡信息用于任何欺诈或非法用途。</p>
              </div>
            </template>
          </el-alert>
        </el-form>
      </el-tab-pane>
      
      <!-- 无感换号标签页 -->
      <el-tab-pane label="无感换号" name="seamless">
        <el-form :model="settings" label-width="140px">
          <el-form-item label="Windsurf路径">
            <el-input
              v-model="windsurfPath"
              placeholder="请输入或点击自动检测获取路径"
              @blur="handlePathChange"
            >
              <template #append>
                <el-button-group>
                  <el-button @click="detectWindsurfPath" :loading="detectingPath">
                    自动检测
                  </el-button>
                  <el-button @click="browseWindsurfPath">
                    浏览
                  </el-button>
                </el-button-group>
              </template>
            </el-input>
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              可手动输入路径或从开始菜单自动检测Windsurf安装路径
            </div>
          </el-form-item>
          
          <el-form-item label="启用无感换号">
            <el-switch 
              v-model="settings.seamlessSwitchEnabled"
              active-text="开启"
              inactive-text="关闭"
              :loading="patchLoading"
              @change="handleSeamlessSwitch"
              :disabled="!windsurfPath"
            />
          </el-form-item>
          
          <el-form-item label="补丁状态">
            <el-tag v-if="patchStatus.installed" type="success">已安装</el-tag>
            <el-tag v-else-if="patchStatus.error" type="danger">{{ patchStatus.error }}</el-tag>
            <el-tag v-else type="info">未安装</el-tag>
            <el-button 
              v-if="patchStatus.installed" 
              size="small" 
              style="margin-left: 10px;"
              @click="checkPatchStatus"
            >
              重新检测
            </el-button>
            <el-button
              v-if="windsurfPath"
              size="small"
              type="warning"
              style="margin-left: 10px;"
              :loading="patchLoading"
              @click="forceReapplyPatch"
            >
              重新打补丁
            </el-button>
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              若首次开启无感换号提示"补丁已经应用过了"但状态仍为"未安装"，可点击"重新打补丁"强制覆盖。
            </div>
          </el-form-item>
          
          <el-form-item label="自动继续补丁">
            <el-space wrap>
              <el-tag v-if="patchStatus.autoContinueBridge" type="success">Bridge补丁已安装</el-tag>
              <el-tag v-else type="info">Bridge补丁未安装</el-tag>
              <el-button
                size="small"
                type="primary"
                :disabled="!windsurfPath"
                :loading="autoContinueBridgeLoading"
                @click="installAutoContinueBridgePatch"
              >
                打补丁
              </el-button>
              <el-button
                size="small"
                type="warning"
                :disabled="!windsurfPath || !patchStatus.autoContinueBridge"
                :loading="autoContinueBridgeLoading"
                @click="restoreAutoContinueBridgePatch"
              >
                还原补丁
              </el-button>
              <el-button
                size="small"
                :loading="autoContinueBridgeLoading"
                @click="refreshAutoContinueBridgePatchStatus"
              >
                检查补丁
              </el-button>
            </el-space>
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              这里仅用于安装或还原 Windsurf 内部 Bridge 补丁；Bridge 的开启/关闭请在分身管理页面的“自动继续工作”区域操作。
            </div>
            <div v-if="autoContinueBridgeMessage" style="margin-top: 5px; color: #67c23a; font-size: 12px;">
              {{ autoContinueBridgeMessage }}
            </div>
          </el-form-item>

          <el-alert
            title="功能说明"
            type="info"
            :closable="false"
            show-icon
            style="margin-top: 20px;"
          >
            <template #default>
              <div style="font-size: 12px; line-height: 1.6;">
                <p>🚀 无感换号功能：实现 Windsurf 账号无感切换</p>
                <p>🔁 自动继续 Bridge：搭配自动换号使用，自动换号切到可用账号后，由 Windsurf 内部 Bridge 捕获页面中的模型异常、额度耗尽或试用用户全局限流等中断事件，并在当前 Cascade 输入框自动填入并提交“继续工作”。</p>
                <p>⚠️ 注意：开启/关闭时会自动重启 Windsurf</p>
              </div>
            </template>
          </el-alert>
          
          <el-divider content-position="left">初始化 Windsurf</el-divider>
          
          <el-form-item label="重置 Windsurf">
            <el-button
              type="danger"
              :loading="resettingWindsurf"
              @click="handleResetWindsurf"
            >
              初始化 Windsurf
            </el-button>
            <div style="margin-top: 5px; color: #909399; font-size: 12px;">
              清除 Windsurf 所有配置、缓存和用户数据，恢复到初始状态
            </div>
          </el-form-item>
          
          <el-alert
            title="初始化说明"
            type="warning"
            :closable="false"
            show-icon
            style="margin-top: 10px;"
          >
            <template #default>
              <div style="font-size: 12px; line-height: 1.6;">
                <p>⚠️ 初始化将清除 Windsurf 的所有配置文件、缓存和用户数据</p>
                <p>⚠️ 初始化后如需使用无感换号功能，需要重新开启</p>
                <p>⚠️ 初始化前请确保 Windsurf 已关闭</p>
              </div>
            </template>
          </el-alert>
        </el-form>
      </el-tab-pane>
    </el-tabs>
    
    <template #footer>
      <el-button @click="handleClose">取消</el-button>
      <el-button type="primary" @click="handleSave" :loading="loading">
        保存
      </el-button>
    </template>
  </el-dialog>
</template>

<script setup lang="ts">
import { ref, reactive, watch, onMounted } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { Connection, RefreshRight } from '@element-plus/icons-vue';
import { useSettingsStore, useUIStore } from '@/store';
import { invoke } from '@tauri-apps/api/core';
import { systemApi } from '@/api';

const settingsStore = useSettingsStore();
const uiStore = useUIStore();

const loading = ref(false);
const activeTab = ref('basic');  // 当前激活的标签页
const seatCountOptionsInput = ref('18, 19, 20');  // 座位数选项输入框
const resettingHttp = ref(false);  // HTTP客户端重置中

// 解析座位数选项
function parseSeatCountOptions() {
  const input = seatCountOptionsInput.value.trim();
  if (!input) {
    settings.seat_count_options = [18, 19, 20];
    seatCountOptionsInput.value = '18, 19, 20';
    return;
  }
  
  const numbers = input.split(/[,，\s]+/)
    .map(s => parseInt(s.trim(), 10))
    .filter(n => !isNaN(n) && n > 0);
  
  if (numbers.length === 0) {
    ElMessage.warning('请输入有效的座位数');
    settings.seat_count_options = [18, 19, 20];
    seatCountOptionsInput.value = '18, 19, 20';
  } else {
    settings.seat_count_options = numbers;
    seatCountOptionsInput.value = numbers.join(', ');
  }
}

const settings = reactive<{
  auto_refresh_token: boolean;
  seat_count_options: number[];
  retry_times: number;
  theme: string;
  concurrent_limit: number;
  show_seats_result_dialog: boolean;
  autoOpenPaymentLinkInWebview: boolean;
  autoFillPaymentForm: boolean;
  autoSubmitPaymentForm: boolean;
  paymentPageDelay: number;
  showVirtualCardInfo: boolean;
  customCardBin: string;
  customCardBinRange: string;
  cardBindRetryTimes: number;
  testModeEnabled: boolean;
  useLocalSuccessBins: boolean;
  seamlessSwitchEnabled: boolean;
  windsurfPath: string | null;
  patchBackupPath: string | null;
  autoContinueBridgeEnabled: boolean;
  autoOpenBrowser: boolean;
  browserMode: 'incognito' | 'normal';
  privacyMode: boolean;
  unlimitedConcurrentRefresh: boolean;
  proxyEnabled: boolean;
  proxyUrl: string | null;
  useLightweightApi: boolean;
  autoSwitchEnabled: boolean;
  autoSwitchGroup: string;
  autoSwitchThreshold: number;
  autoSwitchCheckInterval: number;
  autoSwitchCurrentAccountId: string | null;
  customBrowserPath: string | null;
}>({
  auto_refresh_token: true,
  seat_count_options: [18, 19, 20],
  retry_times: 2,
  theme: 'light',
  concurrent_limit: 5,
  show_seats_result_dialog: false,  // 默认关闭
  autoOpenPaymentLinkInWebview: false,  // 默认关闭自动打开支付页面
  autoFillPaymentForm: false,  // 默认关闭自动填写表单
  autoSubmitPaymentForm: false,  // 默认关闭自动提交
  paymentPageDelay: 2,  // 默认延迟2秒
  showVirtualCardInfo: false,  // 默认关闭虚拟卡信息弹窗
  customCardBin: '626202',  // 默认卡头
  customCardBinRange: '',  // 默认不使用卡段范围
  cardBindRetryTimes: 5,  // 默认绑卡重试5次
  testModeEnabled: false,  // 默认关闭测试模式
  useLocalSuccessBins: false,  // 默认不使用本地BIN池
  seamlessSwitchEnabled: false,  // 默认关闭无感换号
  windsurfPath: null,  // Windsurf路径
  patchBackupPath: null,  // 补丁备份路径
  autoContinueBridgeEnabled: false,
  autoOpenBrowser: true,  // 默认自动打开浏览器
  browserMode: 'incognito',  // 默认无痕模式
  privacyMode: false,  // 默认关闭隐私模式
  unlimitedConcurrentRefresh: false,  // 默认关闭全量并发刷新
  proxyEnabled: false,  // 默认关闭代理
  proxyUrl: null,  // 默认无代理地址
  useLightweightApi: true,  // 默认使用轻量级API
  autoSwitchEnabled: false,  // 默认关闭自动换号
  autoSwitchGroup: '默认分组',
  autoSwitchThreshold: 10,  // 默认10%
  autoSwitchCheckInterval: 300,  // 默认5分钟
  autoSwitchCurrentAccountId: null,
  customBrowserPath: null,  // 默认不设置自定义浏览器路径
});

// 成功BIN池相关
const successBinCount = ref(0);
const testModeProgress = ref<string | null>(null);

async function loadSuccessBinCount() {
  try {
    const bins = await invoke<string[]>('get_success_bins');
    successBinCount.value = bins.length;
  } catch (e) {
    successBinCount.value = 0;
  }
}

async function loadTestModeProgress() {
  try {
    testModeProgress.value = await invoke<string | null>('get_test_mode_progress');
  } catch (e) {
    testModeProgress.value = null;
  }
}

async function resetTestModeProgress() {
  try {
    await ElMessageBox.confirm('确定要重置测试模式进度吗？下次将从范围起始位置开始。', '确认重置', {
      type: 'warning'
    });
    await invoke('reset_test_mode_progress');
    testModeProgress.value = null;
    ElMessage.success('进度已重置');
  } catch (e) {
    // 用户取消
  }
}

async function viewSuccessBins() {
  try {
    const bins = await invoke<string[]>('get_success_bins');
    if (bins.length === 0) {
      ElMessage.info('BIN池为空');
      return;
    }
    ElMessageBox.alert(
      `<div style="max-height: 300px; overflow-y: auto;">
        <p><b>共 ${bins.length} 个成功BIN：</b></p>
        <p style="font-family: monospace; word-break: break-all;">${bins.join(', ')}</p>
      </div>`,
      '成功BIN池',
      { dangerouslyUseHTMLString: true }
    );
  } catch (e) {
    ElMessage.error('获取BIN池失败');
  }
}

async function clearSuccessBins() {
  try {
    await ElMessageBox.confirm('确定要清空所有成功的卡BIN吗？', '确认清空', {
      type: 'warning'
    });
    await invoke('clear_success_bins');
    successBinCount.value = 0;
    ElMessage.success('BIN池已清空');
  } catch (e) {
    // 用户取消
  }
}

// 无感换号相关
const windsurfPath = ref('');
const detectingPath = ref(false);
const patchLoading = ref(false);
const patchStatus = reactive({
  installed: false,
  oauthHandler: false,
  autoContinueBridge: false,
  error: '',
});
const autoContinueBridgeLoading = ref(false);
const autoContinueBridgeMessage = ref('');

// 初始化 Windsurf 相关
const resettingWindsurf = ref(false);

watch(() => uiStore.showSettingsDialog, async (show) => {
  if (show && settingsStore.settings) {
    Object.assign(settings, settingsStore.settings);
    windsurfPath.value = settings.windsurfPath || '';
    // 同步座位数选项到输入框
    if (settings.seat_count_options && settings.seat_count_options.length > 0) {
      seatCountOptionsInput.value = settings.seat_count_options.join(', ');
    }
    // 检查补丁状态
    if (windsurfPath.value) {
      await checkPatchStatus();
    }
    // 加载已安装浏览器列表并同步选中状态
    await loadInstalledBrowsers();
    const currentBrowserPath = settings.customBrowserPath;
    if (currentBrowserPath) {
      const matched = detectedBrowsers.value.find(b => b.path === currentBrowserPath);
      if (matched) {
        selectedBrowserKey.value = matched.path;
        showCustomBrowserInput.value = false;
      } else {
        selectedBrowserKey.value = '__custom__';
        showCustomBrowserInput.value = true;
      }
    } else {
      selectedBrowserKey.value = '';
      showCustomBrowserInput.value = false;
    }
    // 加载成功BIN池数量和测试模式进度
    await loadSuccessBinCount();
    await loadTestModeProgress();
  }
});

onMounted(async () => {
  // 如果已有路径，检查状态
  const storedPath = (settingsStore.settings as any)?.windsurfPath;
  if (storedPath) {
    settings.windsurfPath = storedPath;
    windsurfPath.value = storedPath;
    await checkPatchStatus();
  }
});

async function handleSave() {
  loading.value = true;
  try {
    // 确保保存路径设置
    if (windsurfPath.value) {
      settings.windsurfPath = windsurfPath.value;
    }
    await settingsStore.updateSettings(settings);
    uiStore.setTheme(settings.theme as 'light' | 'dark');
    ElMessage.success('设置保存成功');
    handleClose();
  } catch (error) {
    ElMessage.error(`保存失败: ${error}`);
  } finally {
    loading.value = false;
  }
}

function handleClose() {
  uiStore.showSettingsDialog = false;
}

// 验证卡头输入
function validateCardBin(value: string) {
  // 只允许数字
  const cleaned = value.replace(/[^\d]/g, '');
  settings.customCardBin = cleaned;
  
  // 检查长度
  if (cleaned.length > 0 && cleaned.length < 4) {
    ElMessage.warning('卡头必须是4-12位数字');
  }
}

// 恢复默认卡头
function resetCardBin() {
  settings.customCardBin = '626202';
  ElMessage.success('已恢复默认卡头');
}

// 验证卡段范围格式
function validateCardBinRange(value: string) {
  // 只允许数字和连字符
  const cleaned = value.replace(/[^\d-]/g, '');
  settings.customCardBinRange = cleaned;
  
  // 如果输入了内容，验证格式
  if (cleaned && cleaned.includes('-')) {
    const parts = cleaned.split('-');
    if (parts.length === 2) {
      const [start, end] = parts;
      // 验证两端长度相同且都是数字
      if (start && end && start.length === end.length) {
        const startNum = parseInt(start, 10);
        const endNum = parseInt(end, 10);
        if (startNum > endNum) {
          ElMessage.warning('起始BIN必须小于或等于结束BIN');
        }
      } else if (start && end && start.length !== end.length) {
        ElMessage.warning('起始和结束BIN的长度必须相同');
      }
    }
  }
}

// 清除卡段范围
function clearCardBinRange() {
  settings.customCardBinRange = '';
  ElMessage.success('已清除卡段范围');
}

// 检测Windsurf路径
async function detectWindsurfPath() {
  detectingPath.value = true;
  try {
    const path = await invoke<string>('get_windsurf_path');
    windsurfPath.value = path;
    settings.windsurfPath = path;
    ElMessage.success('已找到Windsurf安装路径');
    // 检查补丁状态
    await checkPatchStatus();
    // 保存路径设置到本地
    await settingsStore.updateSettings(settings);
  } catch (error) {
    ElMessage.error(`检测失败: ${error}`);
    windsurfPath.value = '';
  } finally {
    detectingPath.value = false;
  }
}

// 检查补丁状态
async function checkPatchStatus() {
  if (!windsurfPath.value) return;
  
  try {
    const status = await invoke<any>('check_patch_status', {
      windsurfPath: windsurfPath.value
    });
    patchStatus.installed = status.installed;
    patchStatus.oauthHandler = Boolean(status.oauth_handler);
    patchStatus.autoContinueBridge = Boolean(status.auto_continue_bridge);
    patchStatus.error = status.error || '';
    
    // 同步开关状态与实际补丁状态
    if (status.installed !== settings.seamlessSwitchEnabled) {
      settings.seamlessSwitchEnabled = status.installed;
      // 保存同步后的状态
      await settingsStore.updateSettings(settings);
    }
  } catch (error) {
    patchStatus.installed = false;
    patchStatus.oauthHandler = false;
    patchStatus.autoContinueBridge = false;
    patchStatus.error = error as string;
  }
}

async function installAutoContinueBridgePatch() {
  if (!windsurfPath.value) {
    ElMessage.error('请先检测或设置Windsurf路径');
    return;
  }
  autoContinueBridgeLoading.value = true;
  try {
    const result = await invoke<any>('apply_auto_continue_bridge_patch', {
      windsurfPath: windsurfPath.value,
    });
    if (result.success) {
      ElMessage.success(result.message || 'Bridge补丁已安装');
      await checkPatchStatus();
    }
  } catch (error) {
    ElMessage.error(`安装Bridge补丁失败: ${error}`);
  } finally {
    autoContinueBridgeLoading.value = false;
  }
}

async function restoreAutoContinueBridgePatch() {
  if (!windsurfPath.value) {
    ElMessage.error('请先检测或设置Windsurf路径');
    return;
  }

  try {
    await ElMessageBox.confirm(
      '将只还原自动继续 Bridge 补丁，不会关闭图二里的 Bridge 开关，也不会还原无感换号补丁。还原后会重启 Windsurf，是否继续？',
      '还原 Bridge 补丁',
      {
        confirmButtonText: '确定',
        cancelButtonText: '取消',
        type: 'warning',
      }
    );
  } catch {
    return;
  }

  autoContinueBridgeLoading.value = true;
  try {
    const result = await invoke<any>('restore_auto_continue_bridge_patch', {
      windsurfPath: windsurfPath.value,
    });
    if (result.success) {
      ElMessage.success(result.message || 'Bridge补丁已还原');
      await checkPatchStatus();
    }
  } catch (error) {
    ElMessage.error(`还原Bridge补丁失败: ${error}`);
  } finally {
    autoContinueBridgeLoading.value = false;
  }
}

async function refreshAutoContinueBridgePatchStatus() {
  autoContinueBridgeLoading.value = true;
  try {
    await checkPatchStatus();
    autoContinueBridgeMessage.value = patchStatus.autoContinueBridge
      ? 'Bridge补丁已安装'
      : 'Bridge补丁未安装';
  } catch (error) {
    ElMessage.error(`检查Bridge补丁失败: ${error}`);
  } finally {
    autoContinueBridgeLoading.value = false;
  }
}

// 处理路径变化
function handlePathChange() {
  if (windsurfPath.value) {
    settings.windsurfPath = windsurfPath.value;
    // 检查新路径的补丁状态
    checkPatchStatus();
  }
}

// 浏览器检测相关
interface DetectedBrowser {
  name: string;
  path: string;
}
const detectedBrowsers = ref<DetectedBrowser[]>([]);
const detectingBrowsers = ref(false);
const selectedBrowserKey = ref<string>('');
const showCustomBrowserInput = ref(false);

async function loadInstalledBrowsers() {
  detectingBrowsers.value = true;
  try {
    const browsers = await invoke<DetectedBrowser[]>('detect_installed_browsers');
    detectedBrowsers.value = browsers;
    if (browsers.length === 0) {
      ElMessage.info('未检测到已安装的浏览器');
    }
  } catch (e) {
    console.error('检测浏览器失败:', e);
    detectedBrowsers.value = [];
  } finally {
    detectingBrowsers.value = false;
  }
}

function handleBrowserSelect(val: string) {
  if (val === '__custom__') {
    showCustomBrowserInput.value = true;
    // 不修改 customBrowserPath，让用户手动输入
  } else {
    showCustomBrowserInput.value = false;
    settings.customBrowserPath = val || null;
  }
}

function handleBrowserClear() {
  showCustomBrowserInput.value = false;
  settings.customBrowserPath = null;
  selectedBrowserKey.value = '';
}

// 浏览选择浏览器可执行文件
async function browseBrowserPath() {
  try {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const selected = await open({
      multiple: false,
      title: '选择浏览器可执行文件',
      filters: [
        { name: '可执行文件', extensions: ['exe'] },
        { name: '所有文件', extensions: ['*'] }
      ]
    });
    
    if (selected && typeof selected === 'string') {
      settings.customBrowserPath = selected;
      ElMessage.success('已选择浏览器路径');
    }
  } catch (error) {
    ElMessage.error(`选择文件失败: ${error}`);
  }
}

// 浏览选择路径
async function browseWindsurfPath() {
  try {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const selected = await open({
      directory: true,
      multiple: false,
      title: '选择Windsurf安装目录'
    });
    
    if (selected && typeof selected === 'string') {
      // 验证选择的路径是否包含extension.js文件
      const isValid = await invoke<boolean>('validate_windsurf_path', {
        path: selected
      });
      
      if (isValid) {
        windsurfPath.value = selected;
        settings.windsurfPath = selected;
        ElMessage.success('已选择Windsurf路径');
        await checkPatchStatus();
        // 保存路径设置到本地
        await settingsStore.updateSettings(settings);
      } else {
        ElMessage.error('所选目录不是有效的Windsurf安装目录');
      }
    }
  } catch (error) {
    ElMessage.error(`选择路径失败: ${error}`);
  }
}

// 处理无感换号开关
async function handleSeamlessSwitch(value: boolean) {
  if (!windsurfPath.value) {
    ElMessage.error('请先检测或设置Windsurf路径');
    settings.seamlessSwitchEnabled = !value;
    return;
  }
  
  // 确认对话框
  const action = value ? '开启' : '关闭';
  const message = value 
    ? '开启无感换号将修改Windsurf的extension.js文件并重启Windsurf，是否继续？'
    : '关闭无感换号将还原原始文件并重启Windsurf，是否继续？';
  
  try {
    await ElMessageBox.confirm(
      message,
      `${action}无感换号`,
      {
        confirmButtonText: '确定',
        cancelButtonText: '取消',
        type: 'warning',
      }
    );
  } catch {
    // 用户取消，恢复开关状态
    settings.seamlessSwitchEnabled = !value;
    return;
  }
  
  patchLoading.value = true;
  try {
    let result;
    if (value) {
      // 应用补丁
      result = await invoke<any>('apply_seamless_patch', {
        windsurfPath: windsurfPath.value
      });
    } else {
      // 还原补丁
      result = await invoke<any>('restore_seamless_patch');
    }
    
    if (result.success) {
      ElMessage.success(result.message || `无感换号已${action}`);
      if (result.already_patched) {
        ElMessage.info('补丁已经应用过了');
      }
      // 更新状态
      await checkPatchStatus();
      // 保存设置到本地
      settings.windsurfPath = windsurfPath.value;
      settings.patchBackupPath = result.backup_file || settings.patchBackupPath;
      // 立即保存到本地文件
      await settingsStore.updateSettings(settings);
    } else {
      ElMessage.error(result.message || `${action}失败`);
      settings.seamlessSwitchEnabled = !value;
    }
  } catch (error) {
    ElMessage.error(`${action}失败: ${error}`);
    settings.seamlessSwitchEnabled = !value;
  } finally {
    patchLoading.value = false;
  }
}

// 强制重新打补丁（覆盖）
async function forceReapplyPatch() {
  if (!windsurfPath.value) {
    ElMessage.error('请先检测或设置Windsurf路径');
    return;
  }

  try {
    await ElMessageBox.confirm(
      '将尝试从最干净的备份还原 extension.js 并重新应用补丁，随后会重启 Windsurf。是否继续？',
      '重新打补丁',
      {
        confirmButtonText: '确定',
        cancelButtonText: '取消',
        type: 'warning',
      }
    );
  } catch {
    return;
  }

  patchLoading.value = true;
  try {
    const result = await invoke<any>('apply_seamless_patch', {
      windsurfPath: windsurfPath.value,
      force: true,
    });

    if (result.success) {
      if (result.already_patched) {
        ElMessage.info('补丁已经应用过了');
      } else {
        ElMessage.success(result.message || '补丁已重新应用');
      }
      await checkPatchStatus();
      settings.windsurfPath = windsurfPath.value;
      settings.patchBackupPath = result.backup_file || settings.patchBackupPath;
      await settingsStore.updateSettings(settings);
    } else {
      ElMessage.error(result.message || '重新打补丁失败');
    }
  } catch (error) {
    ElMessage.error(`重新打补丁失败: ${error}`);
  } finally {
    patchLoading.value = false;
  }
}

// 重置HTTP客户端
async function handleResetHttpClient() {
  resettingHttp.value = true;
  try {
    const result = await systemApi.resetHttpClient();
    if (result.success) {
      ElMessage.success(result.message || 'HTTP客户端已重置');
    } else {
      ElMessage.error('重置失败');
    }
  } catch (error) {
    ElMessage.error(`重置失败: ${error}`);
  } finally {
    resettingHttp.value = false;
  }
}

// 初始化 Windsurf
async function handleResetWindsurf() {
  try {
    await ElMessageBox.confirm(
      '此操作将清除 Windsurf 的所有配置文件、缓存和用户数据，恢复到初始状态。\n\n⚠️ 初始化后如需使用无感换号功能，需要重新开启。\n⚠️ 请确保 Windsurf 已关闭。\n\n确定要继续吗？',
      '初始化 Windsurf',
      {
        confirmButtonText: '确定初始化',
        cancelButtonText: '取消',
        type: 'warning',
        confirmButtonClass: 'el-button--danger',
      }
    );
  } catch {
    return;
  }
  
  // 二次确认
  try {
    await ElMessageBox.confirm(
      '⚠️ 再次确认：初始化将不可恢复地删除 Windsurf 所有配置和数据！',
      '二次确认',
      {
        confirmButtonText: '我确定要初始化',
        cancelButtonText: '取消',
        type: 'error',
        confirmButtonClass: 'el-button--danger',
      }
    );
  } catch {
    return;
  }
  
  resettingWindsurf.value = true;
  try {
    const result = await invoke<any>('reset_windsurf');
    if (result.success) {
      ElMessage.success(result.message || 'Windsurf 已初始化');
      // 重置无感换号状态
      settings.seamlessSwitchEnabled = false;
      patchStatus.installed = false;
      patchStatus.error = '';
      await settingsStore.updateSettings(settings);
      ElMessage.warning('如需使用无感换号功能，请重新开启');
    } else {
      ElMessage.error(result.message || '初始化失败');
    }
  } catch (error) {
    ElMessage.error(`初始化失败: ${error}`);
  } finally {
    resettingWindsurf.value = false;
  }
}

// simple 版本已禁用的功能
void parseSeatCountOptions;
</script>

<style scoped>
/* 深色模式样式 */
:deep(.el-dialog) {
  /* 在深色模式下由全局样式控制 */
}

/* 深色模式下的描述文字 */
:root.dark .el-form-item > div[style*="color: #909399"] {
  color: #94a3b8 !important;
}

/* 深色模式下的标签页内容 */
:root.dark .el-tabs__content {
  background-color: transparent;
}

/* 深色模式下的表单项标签 */
:root.dark .el-form-item__label {
  color: #cfd3dc;
}

/* 深色模式下的alert */
:root.dark .el-alert--warning {
  background-color: rgba(230, 162, 60, 0.1);
  border-color: rgba(230, 162, 60, 0.3);
}

:root.dark .el-alert--warning .el-alert__description {
  color: #cfd3dc;
}
</style>
