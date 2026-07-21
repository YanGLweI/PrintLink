<script setup lang="ts">
import { ref, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { ElMessage, ElMessageBox } from "element-plus";

// ===== 状态 =====
const connected = ref(false);
const connecting = ref(false);
const serverAddr = ref("");
const folders = ref<string[]>([]);
const loadingFolders = ref(false);

// 登录表单
const form = ref({
  server_addr: "",
  username: "",
  password: "",
});

// ===== 连接共享盘 =====
async function handleConnect() {
  if (!form.value.server_addr.trim()) {
    ElMessage.warning("请输入服务器地址");
    return;
  }
  if (!form.value.username.trim()) {
    ElMessage.warning("请输入账号");
    return;
  }
  if (!form.value.password) {
    ElMessage.warning("请输入密码");
    return;
  }

  connecting.value = true;
  try {
    const result = await invoke<string[]>("connect_shared_drive", {
      serverAddr: form.value.server_addr.trim(),
      username: form.value.username.trim(),
      password: form.value.password,
    });
    folders.value = result;
    serverAddr.value = form.value.server_addr.trim();
    connected.value = true;
    ElMessage.success(`已连接到 \\\\${serverAddr.value}`);
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    connecting.value = false;
  }
}

// ===== 刷新文件夹列表 =====
async function refreshFolders() {
  loadingFolders.value = true;
  try {
    const result = await invoke<string[]>("get_shared_drive_folders");
    folders.value = result;
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    loadingFolders.value = false;
  }
}

// ===== 打开文件夹 =====
async function openFolder(name: string) {
  const path = `\\\\${serverAddr.value}\\${name}`;
  try {
    await invoke("open_shared_folder", { folderPath: path });
  } catch (e) {
    ElMessage.error(String(e));
  }
}

// ===== 退出登录 =====
async function handleLogout() {
  try {
    await ElMessageBox.confirm(
      "退出后将断开共享盘连接并清除保存的凭据，下次需重新输入。",
      "退出登录",
      { confirmButtonText: "确定退出", cancelButtonText: "取消", type: "warning" }
    );
  } catch {
    return; // 用户取消
  }

  try {
    const msg = await invoke<string>("disconnect_shared_drive");
    ElMessage.success(msg);
    connected.value = false;
    folders.value = [];
    serverAddr.value = "";
    form.value = { server_addr: "", username: "", password: "" };
  } catch (e) {
    ElMessage.error(String(e));
  }
}

// ===== 自动连接（启动时检查已保存配置） =====
onMounted(async () => {
  try {
    const config = await invoke<{ server_addr: string; username: string; password: string } | null>(
      "get_shared_drive_config"
    );
    if (config) {
      // 有已保存配置，自动连接
      connecting.value = true;
      try {
        const result = await invoke<string[]>("connect_shared_drive", {
          serverAddr: config.server_addr,
          username: config.username,
          password: config.password,
        });
        folders.value = result;
        serverAddr.value = config.server_addr;
        form.value = { ...config };
        connected.value = true;
      } catch {
        // 自动连接失败，显示登录表单
        connected.value = false;
      } finally {
        connecting.value = false;
      }
    }
  } catch {
    // 无配置，忽略
  }
});
</script>

<template>
  <div class="panel">
    <!-- 已连接：文件夹列表视图 -->
    <template v-if="connected">
      <div class="panel-toolbar">
        <span class="drive-status">
          <span class="status-dot online"></span>
          <b>\\{{ serverAddr }}</b> 已连接
        </span>
        <div class="toolbar-actions">
          <el-button
            size="small"
            plain
            :loading="loadingFolders"
            @click="refreshFolders"
          >
            <el-icon v-if="!loadingFolders"><Refresh /></el-icon>
            刷新
          </el-button>
          <el-button size="small" type="danger" plain @click="handleLogout">
            <el-icon><SwitchButton /></el-icon>
            退出登录
          </el-button>
        </div>
      </div>

      <div v-loading="loadingFolders" class="folder-list">
        <div
          v-for="name in folders"
          :key="name"
          class="folder-item"
          @click="openFolder(name)"
        >
          <el-icon class="folder-icon" :size="20"><Folder /></el-icon>
          <span class="folder-name">{{ name }}</span>
          <el-icon class="folder-arrow"><ArrowRight /></el-icon>
        </div>

        <el-empty
          v-if="folders.length === 0 && !loadingFolders"
          description="共享盘根目录下没有可见文件夹"
          :image-size="80"
        />
      </div>
    </template>

    <!-- 未连接：登录表单视图 -->
    <template v-else>
      <div v-loading="connecting" class="login-container">
        <div class="login-card">
          <div class="login-icon">
            <el-icon :size="26"><FolderOpened /></el-icon>
          </div>
          <h3 class="login-title">连接共享盘</h3>
          <p class="login-subtitle">输入 SMB 共享盘地址和凭据</p>

          <el-form label-position="top" class="login-form">
            <el-form-item label="服务器地址">
              <el-input
                v-model="form.server_addr"
                placeholder="例如：192.168.1.100"
                clearable
                @keyup.enter="handleConnect"
              />
            </el-form-item>
            <el-form-item label="账号">
              <el-input
                v-model="form.username"
                placeholder="输入用户名"
                clearable
                @keyup.enter="handleConnect"
              />
            </el-form-item>
            <el-form-item label="密码">
              <el-input
                v-model="form.password"
                type="password"
                placeholder="输入密码"
                show-password
                @keyup.enter="handleConnect"
              />
            </el-form-item>
          </el-form>

          <el-button
            type="primary"
            class="login-btn"
            :loading="connecting"
            @click="handleConnect"
          >
            <el-icon v-if="!connecting"><Link /></el-icon>
            连接
          </el-button>
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.panel {
  height: 100%;
  min-height: 0;
  display: flex;
  flex-direction: column;
  background: #fff;
  border-radius: 10px;
  border: 1px solid #e2e8f0;
  padding: 14px 16px;
  box-shadow: 0 1px 3px rgba(15, 23, 42, 0.06);
}

.panel-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
  flex-shrink: 0;
}

.drive-status {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: #334155;
}

.drive-status b {
  font-family: "Consolas", "Courier New", monospace;
  color: #1e4976;
}

.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

.status-dot.online {
  background: #22c55e;
  box-shadow: 0 0 4px rgba(34, 197, 94, 0.5);
}

.toolbar-actions {
  display: flex;
  gap: 8px;
}

/* 文件夹列表 */
.folder-list {
  flex: 1;
  overflow-y: auto;
  min-height: 0;
}

.folder-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 16px;
  border-radius: 6px;
  cursor: pointer;
  transition: background 0.15s ease;
  user-select: none;
}

.folder-item:hover {
  background: #f0f7ff;
}

.folder-item:active {
  background: #dbeafe;
}

.folder-icon {
  color: #f59e0b;
  flex-shrink: 0;
}

.folder-name {
  flex: 1;
  font-size: 14px;
  font-weight: 500;
  color: #1e293b;
}

.folder-arrow {
  color: #94a3b8;
  font-size: 12px;
}

/* 登录表单 */
.login-container {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow-y: auto;
  min-height: 0;
}

.login-card {
  width: 100%;
  max-width: 340px;
  text-align: center;
  padding: 8px 0;
}

.login-icon {
  width: 46px;
  height: 46px;
  border-radius: 12px;
  background: #eff6ff;
  border: 1px solid #bfdbfe;
  display: flex;
  align-items: center;
  justify-content: center;
  margin: 0 auto 10px;
  color: #2563eb;
}

.login-title {
  font-size: 16px;
  font-weight: 700;
  color: #0f172a;
  margin-bottom: 2px;
}

.login-subtitle {
  font-size: 12.5px;
  color: #64748b;
  margin-bottom: 14px;
}

.login-form {
  text-align: left;
}

.login-form :deep(.el-form-item) {
  margin-bottom: 14px;
}

.login-form :deep(.el-form-item__label) {
  font-weight: 500;
  color: #334155;
  padding-bottom: 2px;
  line-height: 1.3;
}

.login-btn {
  width: 100%;
  margin-top: 4px;
  height: 36px;
  font-size: 14px;
}
</style>
