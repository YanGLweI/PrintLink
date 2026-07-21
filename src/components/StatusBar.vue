<script setup lang="ts">
import { computed } from "vue";
import type { StatusState } from "../types/printer";

const props = defineProps<{
  credentialStatus: StatusState;
  serverStatus: StatusState;
  serverAddr: string;
}>();

const credLabel = computed(() => {
  switch (props.credentialStatus) {
    case "ok":
      return "凭据已就绪";
    case "error":
      return "凭据异常";
    default:
      return "凭据检测中";
  }
});

const serverLabel = computed(() => {
  switch (props.serverStatus) {
    case "ok":
      return `${props.serverAddr} 已连接`;
    case "error":
      return "服务器离线";
    default:
      return "连接检测中";
  }
});
</script>

<template>
  <div class="status-bar">
    <div class="status-item">
      <span class="status-dot" :class="`dot-${credentialStatus}`"></span>
      <span class="status-label">{{ credLabel }}</span>
    </div>
    <div class="status-divider"></div>
    <div class="status-item">
      <span class="status-dot" :class="`dot-${serverStatus}`"></span>
      <span class="status-label">{{ serverLabel }}</span>
    </div>
  </div>
</template>

<style scoped>
.status-bar {
  display: flex;
  align-items: center;
  gap: 14px;
  background: rgba(255, 255, 255, 0.08);
  border: 1px solid rgba(255, 255, 255, 0.14);
  border-radius: 999px;
  padding: 7px 16px;
  backdrop-filter: blur(4px);
}

.status-item {
  display: flex;
  align-items: center;
  gap: 7px;
}

.status-label {
  font-size: 12.5px;
  color: rgba(255, 255, 255, 0.88);
  white-space: nowrap;
}

.status-divider {
  width: 1px;
  height: 14px;
  background: rgba(255, 255, 255, 0.25);
}

/* 状态圆点 */
.status-dot {
  width: 9px;
  height: 9px;
  border-radius: 50%;
  flex-shrink: 0;
  position: relative;
}

.dot-ok {
  background: #34d399;
  box-shadow: 0 0 6px rgba(52, 211, 153, 0.8);
}

.dot-ok::after {
  content: "";
  position: absolute;
  inset: -3px;
  border-radius: 50%;
  border: 1px solid rgba(52, 211, 153, 0.5);
  animation: pulse 2s ease-out infinite;
}

.dot-error {
  background: #f87171;
  box-shadow: 0 0 6px rgba(248, 113, 113, 0.8);
}

.dot-checking {
  background: #94a3b8;
  animation: blink 1s ease-in-out infinite;
}

@keyframes pulse {
  0% {
    transform: scale(0.8);
    opacity: 1;
  }
  100% {
    transform: scale(1.6);
    opacity: 0;
  }
}

@keyframes blink {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.3;
  }
}
</style>
