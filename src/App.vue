<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { ElMessage, ElMessageBox } from "element-plus";
import StatusBar from "./components/StatusBar.vue";
import AvailablePrinters from "./components/AvailablePrinters.vue";
import ConnectedPrinters from "./components/ConnectedPrinters.vue";
import type { PrinterItem, LocalPrinterItem, StatusState } from "./types/printer";

// ===== 状态 =====
const credentialStatus = ref<StatusState>("checking");
const serverStatus = ref<StatusState>("checking");
const availablePrinters = ref<PrinterItem[]>([]);
const connectedPrinters = ref<LocalPrinterItem[]>([]);
const loadingAvailable = ref(false);
const loadingConnected = ref(false);
const activeTab = ref("available");
const logMessage = ref("正在初始化...");

let unlistenRefresh: UnlistenFn | null = null;

// ===== 日志 =====
function setLog(msg: string) {
  logMessage.value = msg;
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

// ===== 可连接打印机 =====
async function refreshAvailable() {
  loadingAvailable.value = true;
  serverStatus.value = "checking";
  try {
    const list = await invoke<PrinterItem[]>("get_server_printer_list");
    availablePrinters.value = list;
    serverStatus.value = "ok";
    setLog(`已发现 ${list.length} 台共享打印机`);
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
      <StatusBar :credential-status="credentialStatus" :server-status="serverStatus" />
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
          />
        </el-tab-pane>
      </el-tabs>
    </main>

    <!-- 底部日志栏 -->
    <footer class="app-footer">
      <el-icon class="footer-icon"><InfoFilled /></el-icon>
      <span class="footer-log">{{ logMessage }}</span>
    </footer>
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
  display: flex;
  flex-direction: column;
}

.printer-tabs :deep(.el-tabs__content) {
  flex: 1;
  overflow: hidden;
}

.printer-tabs :deep(.el-tab-pane) {
  height: 100%;
}

.printer-tabs :deep(.el-tabs__header) {
  margin-bottom: 12px;
}

.printer-tabs :deep(.el-tabs__item) {
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

.tab-badge :deep(.el-badge__content) {
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
</style>
<script setup lang="ts">
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

const greetMsg = ref("");
const name = ref("");

async function greet() {
  // Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
  greetMsg.value = await invoke("greet", { name: name.value });
}
</script>

<template>
  <main class="container">
    <h1>Welcome to Tauri + Vue</h1>

    <div class="row">
      <a href="https://vite.dev" target="_blank">
        <img src="/vite.svg" class="logo vite" alt="Vite logo" />
      </a>
      <a href="https://tauri.app" target="_blank">
        <img src="/tauri.svg" class="logo tauri" alt="Tauri logo" />
      </a>
      <a href="https://vuejs.org/" target="_blank">
        <img src="./assets/vue.svg" class="logo vue" alt="Vue logo" />
      </a>
    </div>
    <p>Click on the Tauri, Vite, and Vue logos to learn more.</p>

    <form class="row" @submit.prevent="greet">
      <input id="greet-input" v-model="name" placeholder="Enter a name..." />
      <button type="submit">Greet</button>
    </form>
    <p>{{ greetMsg }}</p>
  </main>
</template>

<style scoped>
.logo.vite:hover {
  filter: drop-shadow(0 0 2em #747bff);
}

.logo.vue:hover {
  filter: drop-shadow(0 0 2em #249b73);
}

</style>
<style>
:root {
  font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
  font-size: 16px;
  line-height: 24px;
  font-weight: 400;

  color: #0f0f0f;
  background-color: #f6f6f6;

  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  -webkit-text-size-adjust: 100%;
}

.container {
  margin: 0;
  padding-top: 10vh;
  display: flex;
  flex-direction: column;
  justify-content: center;
  text-align: center;
}

.logo {
  height: 6em;
  padding: 1.5em;
  will-change: filter;
  transition: 0.75s;
}

.logo.tauri:hover {
  filter: drop-shadow(0 0 2em #24c8db);
}

.row {
  display: flex;
  justify-content: center;
}

a {
  font-weight: 500;
  color: #646cff;
  text-decoration: inherit;
}

a:hover {
  color: #535bf2;
}

h1 {
  text-align: center;
}

input,
button {
  border-radius: 8px;
  border: 1px solid transparent;
  padding: 0.6em 1.2em;
  font-size: 1em;
  font-weight: 500;
  font-family: inherit;
  color: #0f0f0f;
  background-color: #ffffff;
  transition: border-color 0.25s;
  box-shadow: 0 2px 2px rgba(0, 0, 0, 0.2);
}

button {
  cursor: pointer;
}

button:hover {
  border-color: #396cd8;
}
button:active {
  border-color: #396cd8;
  background-color: #e8e8e8;
}

input,
button {
  outline: none;
}

#greet-input {
  margin-right: 5px;
}

@media (prefers-color-scheme: dark) {
  :root {
    color: #f6f6f6;
    background-color: #2f2f2f;
  }

  a:hover {
    color: #24c8db;
  }

  input,
  button {
    color: #ffffff;
    background-color: #0f0f0f98;
  }
  button:active {
    background-color: #0f0f0f69;
  }
}

</style>