<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ElMessage, ElMessageBox } from "element-plus";
import StatusBar from "./components/StatusBar.vue";
import AvailablePrinters from "./components/AvailablePrinters.vue";
import ConnectedPrinters from "./components/ConnectedPrinters.vue";
import SettingsDialog from "./components/SettingsDialog.vue";
import SharedDrive from "./components/SharedDrive.vue";
import type { PrinterItem, LocalPrinterItem, StatusState, AppConfig, DriverInfoUpdate } from "./types/printer";

// ===== 状态 =====
const credentialStatus = ref<StatusState>("checking");
const serverStatus = ref<StatusState>("checking");
const availablePrinters = ref<PrinterItem[]>([]);
const connectedPrinters = ref<LocalPrinterItem[]>([]);
const loadingAvailable = ref(false);
const loadingConnected = ref(false);
// 正在连接中的打印机共享路径（驱动子组件按钮 loading 状态）
const connectingPaths = ref<string[]>([]);
const activeTab = ref("available");
const logMessage = ref("正在初始化...");
// 设置弹窗
const showSettings = ref(false);
const serverAddr = ref("10.60.254.90");

let unlistenRefresh: UnlistenFn | null = null;
let unlistenDriver: UnlistenFn | null = null;
let unlistenDriverComplete: UnlistenFn | null = null;

// ===== 日志 =====
function setLog(msg: string) {
  logMessage.value = msg;
}

// ===== 打开开发者 GitHub 主页 =====
async function openGitHub() {
  try {
    await openUrl("https://yanglwei.github.io/");
  } catch (e) {
    ElMessage.error("无法打开 GitHub 主页");
    setLog(String(e));
  }
}

// ===== 设置保存成功回调 =====
async function handleSettingsSaved(config: AppConfig) {
  serverAddr.value = config.server_addr;
  setLog("配置已更新，正在重新初始化...");
  await initApp();
}

// ===== 初始化 =====
async function initApp() {
  credentialStatus.value = "checking";
  serverStatus.value = "checking";
  setLog("正在写入打印服务器凭据...");

  try {
    const msg = await invoke<string>("init_print_credential");
    credentialStatus.value = "ok";
    setLog(msg);
  } catch (e) {
    credentialStatus.value = "error";
    serverStatus.value = "error";
    const err = String(e);
    setLog(`凭据写入失败：${err}`);
    ElMessage.error(`凭据写入失败：${err}`);
    return;
  }

  await Promise.all([refreshAvailable(), refreshConnected()]);
}

// ===== 可连接打印机（渐进式加载 + 缓存） =====
async function refreshAvailable() {
  loadingAvailable.value = true;
  serverStatus.value = "checking";

  // 1. 尝试读取缓存（秒显）
  try {
    const cached = await invoke<PrinterItem[] | null>("get_printer_cache");
    if (cached && cached.length > 0) {
      availablePrinters.value = cached;
      setLog(`已加载缓存（${cached.length} 台），正在刷新...`);
    }
  } catch { /* 无缓存，忽略 */ }

  // 2. 快速枚举（1-2s，无驱动信息）
  try {
    const list = await invoke<PrinterItem[]>("get_server_printer_list");
    availablePrinters.value = list;
    serverStatus.value = "ok";
    setLog(`已发现 ${list.length} 台共享打印机，正在获取驱动信息...`);

    // 3. 启动后台驱动信息获取
    await invoke("fetch_driver_info_async", { printers: list });
  } catch (e) {
    availablePrinters.value = [];
    serverStatus.value = "error";
    setLog(String(e));
  } finally {
    loadingAvailable.value = false;
  }
}

// ===== 已连接打印机 =====
async function refreshConnected() {
  loadingConnected.value = true;
  try {
    const list = await invoke<LocalPrinterItem[]>("get_local_printer_list");
    connectedPrinters.value = list;
  } catch (e) {
    setLog(String(e));
  } finally {
    loadingConnected.value = false;
  }
}

// ===== 连接打印机 =====
async function handleConnect(printer: PrinterItem) {
  // 重复点击拦截：连接中直接返回
  if (connectingPaths.value.includes(printer.share_path)) return;
  connectingPaths.value.push(printer.share_path);
  try {
    const msg = await invoke<string>("connect_printer", {
      printerPath: printer.share_path,
    });
    ElMessage.success(msg);
    setLog(msg);
    await Promise.all([refreshAvailable(), refreshConnected()]);
  } catch (e) {
    const err = String(e);
    ElMessage.error(err);
    setLog(err);
  } finally {
    // 无论成败，连接结束后才释放 loading，保证状态与真实进度一致
    connectingPaths.value = connectingPaths.value.filter(
      (p) => p !== printer.share_path
    );
  }
}

// ===== 设为默认 =====
async function handleSetDefault(printer: LocalPrinterItem) {
  try {
    const msg = await invoke<string>("set_default_printer", {
      name: printer.name,
    });
    ElMessage.success(msg);
    setLog(msg);
    await refreshConnected();
  } catch (e) {
    const err = String(e);
    ElMessage.error(err);
    setLog(err);
  }
}

// ===== 断开打印机 =====
async function handleDisconnect(printer: LocalPrinterItem) {
  const displayName = printer.name.split("\\").pop() || printer.name;
  try {
    await ElMessageBox.confirm(
      `确定要断开打印机「${displayName}」吗？断开后需重新连接才能使用。`,
      "断开打印机",
      { confirmButtonText: "确定断开", cancelButtonText: "取消", type: "warning" }
    );
  } catch {
    return; // 用户取消
  }

  try {
    const msg = await invoke<string>("remove_printer", { name: printer.name });
    ElMessage.success(msg);
    setLog(msg);
    await Promise.all([refreshAvailable(), refreshConnected()]);
  } catch (e) {
    const err = String(e);
    ElMessage.error(err);
    setLog(err);
  }
}

// ===== 打开属性 =====
async function handleOpenProperty(printer: LocalPrinterItem) {
  try {
    await invoke<string>("open_printer_property", { name: printer.name });
    setLog("已打开打印机属性窗口");
  } catch (e) {
    ElMessage.error(String(e));
  }
}

// ===== 打开首选项 =====
async function handleOpenPreference(printer: LocalPrinterItem) {
  try {
    await invoke<string>("open_printer_preference", { name: printer.name });
    setLog("已打开打印首选项窗口");
  } catch (e) {
    ElMessage.error(String(e));
  }
}

// ===== 生命周期 =====
onMounted(async () => {
  // 加载配置获取服务器地址
  try {
    const config = await invoke<AppConfig>("get_config");
    serverAddr.value = config.server_addr;
  } catch {
    // 配置加载失败使用默认值
  }

  // 监听驱动信息逐条更新事件
  unlistenDriver = await listen<DriverInfoUpdate>("driver-info-updated", (event) => {
    const { share_path, driver_name } = event.payload;
    const idx = availablePrinters.value.findIndex(p => p.share_path === share_path);
    if (idx !== -1) {
      availablePrinters.value[idx] = { ...availablePrinters.value[idx], driver_name };
    }
  });

  // 监听驱动获取完成 → 保存缓存
  unlistenDriverComplete = await listen<number>("driver-info-complete", async () => {
    setLog(`已发现 ${availablePrinters.value.length} 台共享打印机`);
    try {
      await invoke("save_printer_cache", { printers: availablePrinters.value });
    } catch { /* 缓存写入失败不影响功能 */ }
  });

  // 监听托盘刷新事件
  unlistenRefresh = await listen("tray-refresh", () => {
    refreshAvailable();
    refreshConnected();
    setLog("已通过托盘刷新打印机列表");
  });
  await initApp();
});

onUnmounted(() => {
  if (unlistenRefresh) unlistenRefresh();
  if (unlistenDriver) unlistenDriver();
  if (unlistenDriverComplete) unlistenDriverComplete();
});
</script>

<template>
  <div class="app-shell">
    <!-- 顶部标题栏 -->
    <header class="app-header">
      <div class="header-brand">
        <div class="brand-icon">
          <el-icon :size="22"><Printer /></el-icon>
        </div>
        <div class="brand-text">
          <h1>PrintLink</h1>
          <span>共享打印机管理客户端</span>
        </div>
      </div>
      <div class="header-actions">
        <el-tooltip content="设置" placement="bottom">
          <button class="settings-btn" @click="showSettings = true">
            <el-icon :size="18"><Setting /></el-icon>
          </button>
        </el-tooltip>
        <StatusBar
          :credential-status="credentialStatus"
          :server-status="serverStatus"
          :server-addr="serverAddr"
        />
      </div>
    </header>

    <!-- 主体标签页 -->
    <main class="app-main">
      <el-tabs v-model="activeTab" class="printer-tabs">
        <el-tab-pane name="available">
          <template #label>
            <span class="tab-label">
              <el-icon><Monitor /></el-icon>
              可连接打印机
            </span>
          </template>
          <AvailablePrinters
            :printers="availablePrinters"
            :loading="loadingAvailable"
            :connected-names="connectedPrinters.map((p) => p.name.toLowerCase())"
            :connecting-paths="connectingPaths"
            :server-addr="serverAddr"
            @refresh="refreshAvailable"
            @connect="handleConnect"
          />
        </el-tab-pane>

        <el-tab-pane name="connected">
          <template #label>
            <span class="tab-label">
              <el-icon><Connection /></el-icon>
              已连接打印机
              <el-badge
                v-if="connectedPrinters.length > 0"
                :value="connectedPrinters.length"
                :max="99"
                class="tab-badge"
              />
            </span>
          </template>
          <ConnectedPrinters
            :printers="connectedPrinters"
            :loading="loadingConnected"
            @refresh="refreshConnected"
            @set-default="handleSetDefault"
            @disconnect="handleDisconnect"
            @open-property="handleOpenProperty"
            @open-preference="handleOpenPreference"
            @go-available="activeTab = 'available'"
          />
        </el-tab-pane>

        <el-tab-pane name="shared-drive">
          <template #label>
            <span class="tab-label">
              <el-icon><FolderOpened /></el-icon>
              扫描共享盘
            </span>
          </template>
          <SharedDrive />
        </el-tab-pane>
      </el-tabs>
    </main>

    <!-- 底部日志栏 -->
    <footer class="app-footer">
      <el-icon class="footer-icon"><InfoFilled /></el-icon>
      <span class="footer-log">{{ logMessage }}</span>
      <a class="footer-credit" @click="openGitHub">Developed by Yeunglw</a>
    </footer>

    <!-- 设置弹窗 -->
    <SettingsDialog
      v-model:visible="showSettings"
      @saved="handleSettingsSaved"
    />
  </div>
</template>

<style>
/* ===== 全局重置与主题 ===== */
:root {
  --el-color-primary: #2563eb;
  --el-color-primary-light-3: #60a5fa;
  --el-color-primary-light-5: #93c5fd;
  --el-color-primary-light-7: #bfdbfe;
  --el-color-primary-light-8: #dbeafe;
  --el-color-primary-light-9: #eff6ff;
  --el-color-primary-dark-2: #1d4ed8;

  font-family: "Segoe UI", "Microsoft YaHei", system-ui, -apple-system, sans-serif;
  font-size: 14px;
  color: #1e293b;
  background-color: #f1f5f9;
}

* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

html,
body,
#app {
  height: 100%;
  overflow: hidden;
}

/* ===== 应用外壳 ===== */
.app-shell {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: #f1f5f9;
}

/* ===== 顶部标题栏 ===== */
.app-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 20px;
  background: linear-gradient(135deg, #0f2a52 0%, #1e4976 55%, #1a5276 100%);
  color: #fff;
  flex-shrink: 0;
  box-shadow: 0 2px 8px rgba(15, 42, 82, 0.35);
  position: relative;
  z-index: 10;
}

.header-brand {
  display: flex;
  align-items: center;
  gap: 12px;
}

.header-actions {
  display: flex;
  align-items: center;
  gap: 12px;
}

.settings-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 34px;
  height: 34px;
  border-radius: 50%;
  border: none;
  background: transparent;
  color: rgba(255, 255, 255, 0.82);
  cursor: pointer;
  transition: all 0.25s ease;
}

.settings-btn:hover {
  background: rgba(255, 255, 255, 0.14);
  color: #fff;
}

.settings-btn:hover .el-icon {
  transform: rotate(45deg);
}

.settings-btn .el-icon {
  transition: transform 0.3s ease;
}

.brand-icon {
  width: 42px;
  height: 42px;
  border-radius: 10px;
  background: rgba(255, 255, 255, 0.14);
  border: 1px solid rgba(255, 255, 255, 0.22);
  display: flex;
  align-items: center;
  justify-content: center;
  backdrop-filter: blur(4px);
}

.brand-text h1 {
  font-size: 19px;
  font-weight: 700;
  letter-spacing: 0.5px;
  line-height: 1.2;
}

.brand-text span {
  font-size: 12px;
  opacity: 0.72;
  letter-spacing: 0.3px;
}

/* ===== 主体区域 ===== */
.app-main {
  flex: 1;
  overflow: hidden;
  padding: 14px 18px 8px;
  display: flex;
  flex-direction: column;
}

.printer-tabs {
  height: 100%;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.printer-tabs .el-tabs__content {
  flex: 1;
  min-height: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.printer-tabs .el-tab-pane {
  flex: 1;
  min-height: 0;
}

.printer-tabs .el-tabs__header {
  margin-bottom: 12px;
}

.printer-tabs .el-tabs__item {
  font-size: 14px;
  font-weight: 500;
  height: 42px;
}

.tab-label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.tab-badge {
  margin-left: 4px;
}

.tab-badge .el-badge__content {
  top: 2px;
}

/* ===== 底部日志栏 ===== */
.app-footer {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 36px;
  padding: 0 18px;
  background: #0f172a;
  color: #94a3b8;
  font-size: 12.5px;
  flex-shrink: 0;
  border-top: 1px solid #1e293b;
}

.footer-icon {
  color: #38bdf8;
  flex-shrink: 0;
}

.footer-log {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.footer-credit {
  margin-left: auto;
  flex-shrink: 0;
  color: #64748b;
  font-size: 12px;
  cursor: pointer;
  text-decoration: none;
  user-select: none;
  transition: color 0.2s ease;
}

.footer-credit:hover {
  color: #38bdf8;
  text-decoration: underline;
}
</style>
