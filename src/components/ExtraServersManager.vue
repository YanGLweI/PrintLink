<script setup lang="ts">
import { ref, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { ElMessage, ElMessageBox } from "element-plus";
import type { ExtraServerConfig, PrinterItem } from "../types/printer";

const props = defineProps<{
  existingServers?: string[]; // 已存在的服务器地址列表（用于去重提示）
}>();

const emit = defineEmits<{
  serversUpdated: [];
}>();

// ===== 状态 =====
const servers = ref<ExtraServerConfig[]>([]);
const showAddForm = ref(false);
const addingForm = ref({
  server_addr: "",
  username: "",
  password: "",
});
const loadingServer = ref<string | null>(null);
const loading = ref(true);

// ===== 加载服务器列表 =====
async function loadServers() {
  try {
    const result = await invoke<ExtraServerConfig[]>("list_extra_servers");
    servers.value = result;
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    loading.value = false;
  }
}

// ===== 添加服务器 =====
async function handleAddServer() {
  if (!addingForm.value.server_addr.trim()) {
    ElMessage.warning("请输入服务器地址");
    return;
  }
  if (!addingForm.value.username.trim()) {
    ElMessage.warning("请输入用户名");
    return;
  }
  if (!addingForm.value.password) {
    ElMessage.warning("请输入密码");
    return;
  }

  const serverAddr = addingForm.value.server_addr.trim();

  // 检查是否已存在
  if (props?.existingServers?.includes(serverAddr)) {
    ElMessage.warning(`服务器 ${serverAddr} 已被连接，请勿重复添加`);
    return;
  }

  loadingServer.value = serverAddr;
  try {
    const msg = await invoke<string>("add_extra_server", {
      serverAddr,
      username: addingForm.value.username.trim(),
      password: addingForm.value.password,
    });
    ElMessage.success(msg);
    
    // 清空表单
    addingForm.value = {
      server_addr: "",
      username: "",
      password: "",
    };
    showAddForm.value = false;
    
    // 刷新列表并通知父组件
    await loadServers();
    emit("serversUpdated");
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    loadingServer.value = null;
  }
}

// ===== 扫描指定服务器 =====
async function handleScan(server: ExtraServerConfig) {
  try {
    loadingServer.value = server.server_addr;
    
    // scan_cached_server returns PrinterItem[]
    const printers = await invoke<PrinterItem[]>("scan_cached_server", {
      serverAddr: server.server_addr,
    });
    
    const count = printers.length;
    ElMessage.success(`扫描完成，发现 ${count} 台打印机`);
    emit("serversUpdated");
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    loadingServer.value = null;
  }
}

// ===== 删除服务器 =====
async function handleRemoveServer(server: ExtraServerConfig) {
  try {
    await ElMessageBox.confirm(
      `确定要删除服务器 ${server.server_addr} 吗？\n该服务器下的打印机将不再显示。`,
      "删除服务器",
      {
        confirmButtonText: "确定删除",
        cancelButtonText: "取消",
        type: "warning",
      }
    );
    
    await invoke<void>("remove_extra_server", {
      serverAddr: server.server_addr,
    });
    ElMessage.success("服务器已删除");
    await loadServers();
    emit("serversUpdated");
  } catch (e: any) {
    if (e !== "cancelled") {
      ElMessage.error(String(e));
    }
  }
}

// ===== 生命周期 =====
onMounted(() => {
  loadServers();
});
</script>

<template>
  <div class="panel">
    <!-- 顶部操作栏 -->
    <div class="panel-toolbar">
      <span class="hint">
        已添加 <b>{{ servers.length }}</b> 个额外服务器
      </span>
      <el-button type="primary" @click="showAddForm = true">
        <el-icon><Plus /></el-icon>
        添加服务器
      </el-button>
    </div>

    <!-- 服务器列表 -->
    <el-table
      :data="servers"
      stripe
      v-loading="loading"
      class="server-table"
      empty-text="暂无附加服务器，点击按钮添加"
    >
      <el-table-column prop="server_addr" label="服务器地址" min-width="180">
        <template #default="{ row }">
          <span class="mono">{{ row.server_addr }}</span>
        </template>
      </el-table-column>
      <el-table-column prop="username" label="用户名" width="120" />
      <el-table-column label="凭据状态" width="100">
        <template #default="{ row }">
          <el-tag v-if="row.credential_saved" size="small" type="success">
            已保存
          </el-tag>
          <el-tag v-else size="small" type="danger">缺失</el-tag>
        </template>
      </el-table-column>
      <el-table-column label="添加时间" width="160">
        <template #default="{ row }">
          <span>{{ new Date(row.created_at * 1000).toLocaleString("zh-CN") }}</span>
        </template>
      </el-table-column>
      <el-table-column label="操作" width="180" fixed="right">
        <template #default="{ row }">
          <el-button
            size="small"
            :loading="loadingServer === row.server_addr"
            @click="handleScan(row)"
          >
            <el-icon><Refresh /></el-icon>
            扫描
          </el-button>
          <el-button
            size="small"
            type="danger"
            @click="handleRemoveServer(row)"
          >
            <el-icon><Delete /></el-icon>
            删除
          </el-button>
        </template>
      </el-table-column>
    </el-table>

    <!-- 添加弹窗 -->
    <el-dialog v-model="showAddForm" title="添加 SMB 服务器" width="420px">
      <el-form :model="addingForm" label-width="80px">
        <el-form-item label="服务器地址">
          <el-input
            v-model="addingForm.server_addr"
            placeholder="例如：192.168.1.100"
            clearable
            @keyup.enter="handleAddServer"
          />
        </el-form-item>
        <el-form-item label="用户名">
          <el-input
            v-model="addingForm.username"
            placeholder="SMB 访问账号"
            clearable
            @keyup.enter="handleAddServer"
          />
        </el-form-item>
        <el-form-item label="密码">
          <el-input
            v-model="addingForm.password"
            type="password"
            placeholder="输入密码"
            show-password
            @keyup.enter="handleAddServer"
          />
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="showAddForm = false">取消</el-button>
        <el-button
          type="primary"
          @click="handleAddServer"
          :loading="!!loadingServer"
        >
          添加并扫描
        </el-button>
      </template>
    </el-dialog>
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

.hint {
  font-size: 13px;
  color: #64748b;
}

.hint b {
  color: #2563eb;
  font-weight: 600;
}

.server-table {
  flex: 1;
  --el-table-border-color: #eef2f7;
  --el-table-header-bg-color: #f8fafc;
}

.mono {
  font-family: "Consolas", "Courier New", monospace;
  font-size: 12.5px;
  color: #475569;
}
</style>
