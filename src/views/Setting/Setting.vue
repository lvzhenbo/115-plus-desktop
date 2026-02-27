<template>
  <div class="p-6">
    <NTabs type="segment" animated>
      <NTabPane name="videoPlayerSetting" tab="视频播放器设置">
        <NForm label-placement="left" label-width="auto" :show-feedback="false">
          <NFormItem label="默认播放音量" path="videoPlayerSetting.defaultVolume">
            <NSlider
              v-model:value="settingStore.videoPlayerSetting.defaultVolume"
              :step="0.01"
              :max="1"
              :format-tooltip
            />
          </NFormItem>
          <NFormItem label="默认播放速度" path="videoPlayerSetting.defaultRate">
            <NRadioGroup
              v-model:value="settingStore.videoPlayerSetting.defaultRate"
              name="radiogroup"
            >
              <NSpace>
                <NRadio :value="0.5"> 0.5x </NRadio>
                <NRadio :value="0.75"> 0.75x </NRadio>
                <NRadio :value="1"> 1x </NRadio>
                <NRadio :value="1.25"> 1.25x </NRadio>
                <NRadio :value="1.5"> 1.5x </NRadio>
                <NRadio :value="2"> 2x </NRadio>
                <NRadio :value="3"> 3x </NRadio>
                <NRadio :value="4"> 4x </NRadio>
                <NRadio :value="5"> 5x </NRadio>
              </NSpace>
            </NRadioGroup>
          </NFormItem>
          <NFormItem label="是否自动播放" path="videoPlayerSetting.autoPlay">
            <NSwitch v-model:value="settingStore.videoPlayerSetting.autoPlay" />
          </NFormItem>
          <NFormItem label="是否同步播放进度" path="videoPlayerSetting.isHistory">
            <NSwitch v-model:value="settingStore.videoPlayerSetting.isHistory" />
          </NFormItem>
        </NForm>
      </NTabPane>
      <NTabPane name="cloudDownloadSetting" tab="云下载设置">
        <NForm label-placement="left" label-width="auto" :show-feedback="false">
          <NFormItem label="默认删除源文件" path="cloudDownloadSetting.deleteSourceFile">
            <NSwitch v-model:value="settingStore.cloudDownloadSetting.deleteSourceFile" />
          </NFormItem>
        </NForm>
      </NTabPane>
      <NTabPane name="downloadSetting" tab="下载设置">
        <NForm label-placement="left" label-width="auto" :show-feedback="false">
          <NFormItem label="下载目录" path="downloadSetting.downloadPath">
            <NInputGroup>
              <NInput v-model:value="settingStore.downloadSetting.downloadPath" readonly />
              <NButton type="primary" @click="selectDownloadDirectory"> 选择下载目录 </NButton>
            </NInputGroup>
          </NFormItem>
          <NFormItem label="启动时自动恢复未完成下载" path="downloadSetting.autoResumeDownloads">
            <NSwitch v-model:value="settingStore.downloadSetting.autoResumeDownloads" />
          </NFormItem>
        </NForm>
      </NTabPane>
    </NTabs>
  </div>
</template>

<script setup lang="ts">
  import { useSettingStore } from '@/store/setting';
  import type { SliderProps } from 'naive-ui';
  import { open } from '@tauri-apps/plugin-dialog';

  const settingStore = useSettingStore();

  const formatTooltip: SliderProps['formatTooltip'] = (v) => `${(v * 100).toFixed(0)}%`;

  const selectDownloadDirectory = async () => {
    const dir = await open({
      multiple: false,
      directory: true,
    });
    if (dir) {
      settingStore.downloadSetting.downloadPath = dir;
    }
  };
</script>

<style scoped></style>
