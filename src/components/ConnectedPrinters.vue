<script setup lang="ts">
import type { LocalPrinterItem } from "../types/printer";

defineProps<{
  printers: LocalPrinterItem[];
  loading: boolean;
}>();

const emit = defineEmits<{
  refresh: [];
  "set-default": [printer: LocalPrinterItem];
  disconnect: [printer: LocalPrinterItem];
  "open-property": [printer: LocalPrinterItem];
  "open-preference": [printer: LocalPrinterItem];
  "go-available": [];
}>();

function displayName(name: string): string {
  return name.split("\\").pop() || name;
}
</script>

<template>
  <div class="panel">
    <div class="panel-toolbar">
      <span class="panel-hint">
        本机已安装的打印服务器设备，可管理属性、首选项与默认设置
      </span>
      <el-button
        type="primary"
        plain
        size="small"
        :loading="loading"
        @click="emit('refresh')"
      >
        <el-icon v-if="!loading"><Refresh /></el-icon>
        刷新状态
      </el-button>
    </div>

    <el-table
      v-if="printers.length > 0"
      v-loading="loading"
      :data="printers"
      height="100%"
      stripe
      class="printer-table"
    >
      <el-table-column label="打印机名称" min-width="150">
        <template #default="{ row }">
          <div class="printer-name-cell">
            <el-icon class="printer-icon"><Printer /></el-icon>
            <span>{{ displayName(row.name) }}</span>
            <el-tag
              v-if="row.is_default"
              size="small"
              type="warning"
              effect="dark"
              class="default-tag"
            >
              默认
            </el-tag>
          </div>
        </template>
      </el-table-column>
      <el-table-column label="端口" min-width="175">
        <template #default="{ row }">
          <span class="mono">{{ row.port_name }}</span>
        </template>
      </el-table-column>
      <el-table-column prop="driver_name" label="驱动" min-width="135" />
      <el-table-column label="状态" width="72" align="center">
        <template #default="{ row }">
          <el-tag size="small" type="success" effect="light">{{ row.status }}</el-tag>
        </template>
      </el-table-column>
      <el-table-column label="操作" width="218" align="center" fixed="right">
        <template #default="{ row }">
          <div class="action-group">
            <el-button size="small" @click="emit('open-property', row)">属性</el-button>
            <el-button size="small" @click="emit('open-preference', row)">首选项</el-button>
            <el-button
              v-if="!row.is_default"
              size="small"
              type="warning"
              plain
              @click="emit('set-default', row)"
            >
              设为默认
            </el-button>
            <el-button
              v-else
              size="small"
              type="success"
              plain
              disabled
            >
              当前默认
            </el-button>
            <el-button
              size="small"
              type="danger"
              plain
              @click="emit('disconnect', row)"
            >
              断开
            </el-button>
          </div>
        </template>
      </el-table-column>

    </el-table>

    <div v-else v-loading="loading" class="empty-state">
      <el-empty description="尚未连接任何共享打印机" :image-size="110">
        <el-button type="primary" @click="emit('go-available')">
          <span class="button-content">
            <el-icon><Monitor /></el-icon>
            连接打印机
          </span>
        </el-button>
      </el-empty>
    </div>
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

.panel-hint {
  font-size: 13px;
  color: #64748b;
}

.printer-table {
  flex: 1;
  --el-table-border-color: #eef2f7;
  --el-table-header-bg-color: #f8fafc;
}

.empty-state {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
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

.default-tag {
  margin-left: 2px;
}

.mono {
  font-family: "Consolas", "Courier New", monospace;
  font-size: 12.5px;
  color: #475569;
}

.action-group {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 6px;
  width: 100%;
}

.action-group :deep(.el-button) {
  margin-left: 0;
  width: 100%;
}

.action-group :deep(.el-button + .el-button) {
  margin-left: 0;
}
.button-content {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
</style>
