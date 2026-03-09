<template>
  <div class="p-4">
    <NTabs type="segment" animated>
      <NTabPane name="generalSetting" tab="常规设置">
        <NForm label-placement="left" label-width="auto">
          <NFormItem label="排序方式" path="generalSetting.customOrder">
            <NRadioGroup
              v-model:value="settingStore.generalSetting.customOrder"
              name="customOrderGroup"
            >
              <NSpace>
                <NRadio :value="0"> 记忆排序 </NRadio>
                <NRadio :value="1"> 自定义排序 </NRadio>
                <NRadio :value="2"> 自定义排序（非文件夹置顶） </NRadio>
              </NSpace>
            </NRadioGroup>
          </NFormItem>
          <NFormItem label="退出时有传输任务不再提示" path="generalSetting.skipExitConfirm">
            <NSwitch v-model:value="settingStore.generalSetting.skipExitConfirm" />
          </NFormItem>
          <NFormItem label="启动时自动检查更新" path="generalSetting.autoCheckUpdate">
            <NSwitch v-model:value="settingStore.generalSetting.autoCheckUpdate" />
          </NFormItem>
          <NFormItem label="更新代理地址" path="generalSetting.updateProxy">
            <NInput
              v-model:value="settingStore.generalSetting.updateProxy"
              placeholder="http://127.0.0.1:7890"
              clearable
            />
          </NFormItem>
          <NFormItem label="接口速率限制" path="generalSetting.apiRateLimit">
            <NInputNumber
              v-model:value="settingStore.generalSetting.apiRateLimit"
              :min="0"
              :max="20"
              :step="1"
              placeholder="0为不限制"
            >
              <template #suffix> 次/秒 </template>
            </NInputNumber>
          </NFormItem>
        </NForm>
      </NTabPane>
      <NTabPane name="videoPlayerSetting" tab="视频播放器设置">
        <NScrollbar class="max-h-[calc(100vh-142px)]">
          <NForm label-placement="left" label-width="auto">
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
            <NDivider> 字幕样式 </NDivider>
            <NFormItem label="默认开启字幕" path="subtitleStyleSetting.defaultEnabled">
              <NSwitch v-model:value="settingStore.subtitleStyleSetting.defaultEnabled" />
            </NFormItem>
            <NFormItem label="字体大小" path="subtitleStyleSetting.fontSize">
              <NInputNumber
                v-model:value="settingStore.subtitleStyleSetting.fontSize"
                :min="12"
                :max="48"
                :step="1"
              >
                <template #suffix> px </template>
              </NInputNumber>
            </NFormItem>
            <NFormItem label="字体颜色" path="subtitleStyleSetting.fontColor">
              <NColorPicker
                v-model:value="settingStore.subtitleStyleSetting.fontColor"
                :modes="['hex']"
                :show-alpha="false"
              />
            </NFormItem>
            <NFormItem label="加粗" path="subtitleStyleSetting.fontBold">
              <NSwitch v-model:value="settingStore.subtitleStyleSetting.fontBold" />
            </NFormItem>
            <NFormItem label="描边颜色" path="subtitleStyleSetting.strokeColor">
              <NColorPicker
                v-model:value="settingStore.subtitleStyleSetting.strokeColor"
                :modes="['hex']"
                :show-alpha="false"
              />
            </NFormItem>
            <NFormItem label="描边宽度" path="subtitleStyleSetting.strokeWidth">
              <NInputNumber
                v-model:value="settingStore.subtitleStyleSetting.strokeWidth"
                :min="0"
                :max="5"
                :step="0.5"
              >
                <template #suffix> px </template>
              </NInputNumber>
            </NFormItem>
            <NFormItem label="背景颜色" path="subtitleStyleSetting.backgroundColor">
              <NColorPicker
                v-model:value="settingStore.subtitleStyleSetting.backgroundColor"
                :modes="['hex']"
              />
            </NFormItem>
            <NFormItem label="距底部偏移" path="subtitleStyleSetting.bottomOffset">
              <NInputNumber
                v-model:value="settingStore.subtitleStyleSetting.bottomOffset"
                :min="0"
                :max="200"
                :step="4"
              >
                <template #suffix> px </template>
              </NInputNumber>
            </NFormItem>
            <NFormItem label="预览">
              <div
                class="w-full h-30 bg-zinc-800 rounded flex items-end justify-center overflow-hidden relative"
              >
                <div
                  class="inline-flex flex-col items-center gap-0.5 mb-2"
                  :style="{
                    marginBottom: `${Math.min(settingStore.subtitleStyleSetting.bottomOffset / 4, 40)}px`,
                  }"
                >
                  <span
                    class="inline-block px-3 py-0.5 rounded text-center leading-relaxed"
                    :style="subtitlePreviewStyle"
                  >
                    示例字幕 Sample Subtitle
                  </span>
                </div>
              </div>
            </NFormItem>
          </NForm>
        </NScrollbar>
      </NTabPane>
      <NTabPane name="cloudDownloadSetting" tab="云下载设置">
        <NForm label-placement="left" label-width="auto">
          <NFormItem label="默认删除源文件" path="cloudDownloadSetting.deleteSourceFile">
            <NSwitch v-model:value="settingStore.cloudDownloadSetting.deleteSourceFile" />
          </NFormItem>
        </NForm>
      </NTabPane>
      <NTabPane name="downloadSetting" tab="下载设置">
        <NForm label-placement="left" label-width="auto">
          <NFormItem label="下载目录" path="downloadSetting.downloadPath">
            <NInputGroup>
              <NInput v-model:value="settingStore.downloadSetting.downloadPath" readonly />
              <NButton type="primary" @click="selectDownloadDirectory"> 选择下载目录 </NButton>
            </NInputGroup>
          </NFormItem>
          <NFormItem label="最大重试次数" path="downloadSetting.maxRetry">
            <NInputNumber
              v-model:value="settingStore.downloadSetting.maxRetry"
              :min="0"
              :max="20"
              :step="1"
            />
          </NFormItem>
          <NFormItem label="并行任务数" path="downloadSetting.maxConcurrent">
            <NInputNumber
              v-model:value="settingStore.downloadSetting.maxConcurrent"
              :min="1"
              :max="10"
              :step="1"
            />
          </NFormItem>
        </NForm>
      </NTabPane>
      <NTabPane name="uploadSetting" tab="上传设置">
        <NForm label-placement="left" label-width="auto">
          <NFormItem label="最大重试次数" path="uploadSetting.maxRetry">
            <NInputNumber
              v-model:value="settingStore.uploadSetting.maxRetry"
              :min="0"
              :max="20"
              :step="1"
            />
          </NFormItem>
          <NFormItem label="并行任务数" path="uploadSetting.maxConcurrent">
            <NInputNumber
              v-model:value="settingStore.uploadSetting.maxConcurrent"
              :min="1"
              :max="10"
              :step="1"
            />
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
  import { generateTextShadow } from '@/utils/subtitleStyleUtils';
  import type { CSSProperties } from 'vue';

  const settingStore = useSettingStore();

  const formatTooltip: SliderProps['formatTooltip'] = (v) => `${(v * 100).toFixed(0)}%`;

  /** 字幕预览样式 */
  const subtitlePreviewStyle = computed<CSSProperties>(() => {
    const s = settingStore.subtitleStyleSetting;
    return {
      color: s.fontColor,
      fontSize: `${s.fontSize}px`,
      fontWeight: s.fontBold ? 'bold' : 'normal',
      backgroundColor: s.backgroundColor,
      textShadow: generateTextShadow(s.strokeColor, s.strokeWidth),
    };
  });

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
