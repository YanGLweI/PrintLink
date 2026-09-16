<script setup lang="ts">
import { ref, computed } from "vue";
import type { PrinterItem, ExtraServerConfig } from "../types/printer";

const props = defineProps<{
  printers: PrinterItem[];
  loading: boolean;
  connectedNames: string[];
  /** 正在连接中的打印机共享路径（由父组件根据真实连接进度维护） */
  connectingPaths: string[];
  /** 当前配置的打印服务器地址 */
  serverAddr: string;
  /** 附加服务器列表（用于筛选器） */
  extraServers: ExtraServerConfig[];
}>();

const emit = defineEmits<{
  refresh: [];
  connect: [printer: PrinterItem];
}>();

function isConnecting(printer: PrinterItem): boolean {
  return props.connectingPaths.includes(printer.share_path);
}

function handleConnect(printer: PrinterItem) {
  // 连接中则拦截重复点击；loading 状态由父组件的真实连接进度驱动
  if (isConnecting(printer)) return;
  emit("connect", printer);
}

function isConnected(printer: PrinterItem): boolean {
  return props.connectedNames.includes(printer.share_path.toLowerCase());
}

// ===== 服务器筛选 =====
const filterServer = ref<string>(""); // 空字符串表示全部

// 计算属性：按服务器分组
const groupedPrinters = computed(() => {
  const filtered = filterServer.value
    ? props.printers.filter((p) => p.server_addr === filterServer.value)
    : props.printers;

  // 按服务器地址分组
  const groups: Record<string, PrinterItem[]> = {};
  for (const printer of filtered) {
    if (!groups[printer.server_addr]) {
      groups[printer.server_addr] = [];
    }
    groups[printer.server_addr].push(printer);
  }
  return groups;
});

function getSrvLabel(addr: string): string {
  if (addr === props.serverAddr) return "主打印服务器";
  return `附加服务器：${addr}`;
}

function getShortLabel(addr: string): string {
  if (addr === props.serverAddr) return "主服务器";
  // 显示最后一段 IP 作简写
  return addr.split(".").pop() || addr;
}
</script>

<template>
  <div class="panel">
    <div class="panel-toolbar">
      <!-- 服务器筛选器 -->
      <el-select
        v-model="filterServer"
        placeholder="按服务器筛选"
        clearable
        style="width: 200px"
      >
        <el-option label="全部服务器" value="" />
        <el-option :value="serverAddr" :label="'主服务器 · ' + serverAddr" />
        <el-option
          v-for="srv in extraServers"
          :key="srv.server_addr"
          :value="srv.server_addr"
          :label="'附加服务器 · ' + srv.server_addr"
        />
      </el-select>

      <span class="panel-hint">
        共 <b>{{ printers.length }}</b> 台打印机
      </span>
      <el-button
        type="primary"
        plain
        size="small"
        :loading="loading"
        @click="$emit('refresh')"
      >
        <el-icon v-if="!loading"><Refresh /></el-icon>
        刷新全部
      </el-button>
    </div>

    <!-- 分组展示区域 -->
    <div
      v-for="(group, srvAddr) in groupedPrinters"
      :key="srvAddr"
      class="server-group"
    >
      <div class="server-group-header">
        <el-icon><FolderOpened /></el-icon>
        <b>{{ getSrvLabel(srvAddr) }}</b>
        <span class="count">{{ group.length }} 台</span>
      </div>

      <el-table
        v-loading="loading"
        :data="group"
        stripe
        class="printer-table"
        empty-text="暂无打印机，该服务器可能已离线"
      >
        <el-table-column prop="name" label="打印机名称" min-width="160">
          <template #default="{ row }">
            <div class="printer-name-cell">
              <el-icon class="printer-icon"><Printer /></el-icon>
              <span>{{ row.name }}</span>
            </div>
          </template>
        </el-table-column>
        <el-table-column prop="share_path" label="共享路径" min-width="220">
          <template #default="{ row }">
            <div class="share-path-cell">
              <span class="mono">{{ row.share_path }}</span>
              <el-tag
                v-if="row.server_addr !== serverAddr"
                size="small"
                type="info"
              >
                {{ getShortLabel(row.server_addr) }}
              </el-tag>
            </div>
          </template>
        </el-table-column>
        <el-table-column prop="driver_name" label="驱动名称" min-width="160">
          <template #default="{ row }">
            <span :class="{ 'driver-pending': row.driver_name === '连接后自动识别' }">
              {{ row.driver_name }}
            </span>
          </template>
        </el-table-column>
        <el-table-column label="状态" width="80" align="center">
          <template #default="{ row }">
            <el-tag size="small" type="success" effect="light">{{ row.status }}</el-tag>
          </template>
        </el-table-column>
        <el-table-column label="操作" width="110" align="center" fixed="right">
          <template #default="{ row }">
            <el-button
              v-if="isConnected(row)"
              size="small"
              type="info"
              plain
              disabled
            >
              已安装
            </el-button>
            <el-button
              v-else
              size="small"
              type="primary"
              :loading="isConnecting(row)"
              @click="handleConnect(row)"
            >
              {{ isConnecting(row) ? "连接中" : "连接" }}
            </el-button>
          </template>
        </el-table-column>

        <template #empty>
          <el-empty
            :description="`暂无打印机，该服务器可能已离线`
            "
            :image-size="90"
          />
        </template>
      </el-table>
    </div>

    <!-- 空状态：无打印机或无匹配服务器 -->
    <el-empty
      v-if="Object.keys(groupedPrinters).length === 0 && !loading"
      description="暂无可连接打印机，请确认网络或点击刷新"
      :image-size="90"
    />
  </div>
</template>

<style scoped>
.panel {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: #fff;
  border-radius: 10px;
  border: 1px solid #e2e8f0;
  padding: 14px 16px;
  box-shadow: 0 1px 3px rgba(15, 23, 42, 0.06);
  overflow-y: auto; /* 添加垂直滚动 */
}

.panel-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between; /* 减小组件与表格之间的间距 */
  flex-shrink: 0;
}

.panel-hint {
  font-size: 13px;
  color: #64748b;
}

.panel-hint b {
  color: #2563eb;
  font-weight: 600;
}

/* 服务器分组标题 */
.server-group-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 14px 12px 12px;
  font-size: 13px;
  color: #64748b;
  border-bottom: 1px dashed #e2e8f0;
  margin-top: 20px;
  margin-bottom: 8px;
}

.server-group-header b {
  color: #1e293b;
  font-weight: 600;
  font-size: 14px;
}

.printer-table {
  --el-table-border-color: #eef2f7;
  --el-table-header-bg-color: #f8fafc;
}

.printer-name-cell {
  display: flex;
  align-items: center;
  gap: 8px;
  font-weight: 500;
  color: #0f172a;
}

.printer-icon {
  color: #2563eb;
  flex-shrink: 0;
}

.count {
  margin-left: auto;
  font-size: 12px;
  color: #94a3b8;
}

.mono {
  font-family: "Consolas", "Courier New", monospace;
  font-size: 12.5px;
  color: #475569;
}

/* 分享路径单元格（带服务器标签） */
.share-path-cell {
  display: flex;
  align-items: center;
  gap: 8px;
}

.server-group {
  
}

.driver-pending {
  color: #999;
  font-style: italic;
}
</style>
