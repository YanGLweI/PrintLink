<script setup lang="ts">
import { ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { ElMessage } from "element-plus";
import type { AppConfig } from "../types/printer";

const props = defineProps<{
  visible: boolean;
}>();

const emit = defineEmits<{
  "update:visible": [value: boolean];
  saved: [config: AppConfig];
}>();

// 表单数据
const form = ref<AppConfig>({
  server_addr: "",
  username: "",
  password: "",
});

const saving = ref(false);
const resetting = ref(false);

// 弹窗打开时加载当前配置
watch(
  () => props.visible,
  async (val) => {
    if (val) {
      try {
        const config = await invoke<AppConfig>("get_config");
        form.value = { ...config };
      } catch (e) {
        ElMessage.error("加载配置失败：" + String(e));
      }
    }
  }
);

function handleClose() {
  emit("update:visible", false);
}

// 保存配置
async function handleSave() {
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

  saving.value = true;
  try {
    const msg = await invoke<string>("save_config_command", {
      serverAddr: form.value.server_addr.trim(),
      username: form.value.username.trim(),
      password: form.value.password,
    });
    ElMessage.success(msg);
    emit("saved", { ...form.value });
    emit("update:visible", false);
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    saving.value = false;
  }
}

// 恢复默认配置
async function handleReset() {
  resetting.value = true;
  try {
    const defaultConfig = await invoke<AppConfig>("reset_config");
    form.value = { ...defaultConfig };
    ElMessage.success("已恢复默认配置，点击「保存」后生效");
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    resetting.value = false;
  }
}
</script>

<template>
  <el-dialog
    :model-value="visible"
    title="打印服务器设置"
    width="420px"
    :close-on-click-modal="false"
    @update:model-value="handleClose"
  >
    <el-form label-position="top" class="settings-form">
      <el-form-item label="服务器地址">
        <el-input
          v-model="form.server_addr"
          placeholder="例如：10.60.254.90"
          clearable
        >
          <template #prefix>
            <el-icon><Monitor /></el-icon>
          </template>
        </el-input>
      </el-form-item>

      <el-form-item label="账号">
        <el-input
          v-model="form.username"
          placeholder="打印服务器登录账号"
          clearable
        >
          <template #prefix>
            <el-icon><User /></el-icon>
          </template>
        </el-input>
      </el-form-item>

      <el-form-item label="密码">
        <el-input
          v-model="form.password"
          type="password"
          placeholder="打印服务器登录密码"
          show-password
        >
          <template #prefix>
            <el-icon><Lock /></el-icon>
          </template>
        </el-input>
      </el-form-item>
    </el-form>

    <template #footer>
      <div class="dialog-footer">
        <el-button
          plain
          :loading="resetting"
          @click="handleReset"
        >
          恢复默认配置
        </el-button>
        <el-button
          type="primary"
          :loading="saving"
          @click="handleSave"
        >
          保存
        </el-button>
      </div>
    </template>
  </el-dialog>
</template>

<style scoped>
.settings-form {
  padding: 4px 0;
}

.settings-form :deep(.el-form-item__label) {
  font-weight: 500;
  color: #334155;
}

.dialog-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
</style>
