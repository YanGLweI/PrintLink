<script setup lang="ts">
import { ref } from "vue";
import type { PrinterItem } from "../types/printer";

defineProps<{
  printers: PrinterItem[];
  loading: boolean;
  connectedNames: string[];
}>();

const emit = defineEmits<{
  refresh: [];
  connect: [printer: PrinterItem];
}>();

// 正在连接的打印机路径集合（防止重复点击）
const connectingPaths = ref<Set<string>>(new Set());

async function handleConnect(printer: PrinterItem) {
  if (connectingPaths.value.has(printer.share_path)) return;
  connectingPaths.value.add(printer.share_path);
  try {
    emit("connect", printer);
    // 等待一小段时间让父组件处理，实际 loading 由父组件的 refresh 控制
    await new Promise((r) => setTimeout(r, 1500));
  } finally {
    connectingPaths.value.delete(printer.share_path);
  }
}

function isConnected(printer: PrinterItem): boolean {
  return connectedNames.includes(printer.share_path.toLowerCase());
}
</script>

<template>
  <div class="panel">
    <div class="panel-toolbar">
      <span class="panel-hint">
        打印服务器 <b>\\10.60.254.90</b> 下的共享打印机，点击「连接」一键安装
      </span>
      <el-button
        type="primary"
        plain
        size="small"
        :loading="loading"
        @click="emit('refresh')"
      >
        <el-icon v-if="!loading"><Refresh /></el-icon>
        刷新列表
      </el-button>
    </div>

    <el-table
      v-loading="loading"
      :data="printers"
      height="100%"
      stripe
      class="printer-table"
      empty-text=" "
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
          <span class="mono">{{ row.share_path }}</span>
        </template>
      </el-table-column>
      <el-table-column prop="driver_name" label="驱动名称" min-width="160" />
      <el-table-column prop="status" label="状态" width="80" align="center">
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
            :loading="connectingPaths.has(row.share_path)"
            @click="handleConnect(row)"
          >
            连接
          </el-button>
        </template>
      </el-table-column>

      <template #empty>
        <el-empty
          description="暂无共享打印设备，请确认服务器已开启共享或点击刷新"
          :image-size="90"
        />
      </template>
    </el-table>
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
}

.panel-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
  flex-shrink: 0;
}

.panel-hint {
  font-size: 13px;
  color: #64748b;
}

.panel-hint b {
  color: #1e4976;
  font-family: "Consolas", "Courier New", monospace;
  font-weight: 600;
}

.printer-table {
  flex: 1;
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

.mono {
  font-family: "Consolas", "Courier New", monospace;
  font-size: 12.5px;
  color: #475569;
}
</style>
