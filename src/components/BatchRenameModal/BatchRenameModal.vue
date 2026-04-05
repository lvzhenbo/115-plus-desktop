<template>
  <NModal v-model:show="show" preset="card" class="w-[80vw]!" title="批量重命名">
    <NAlert title="注意" type="warning" class="mb-2">
      由于115并没有提供批量重命名的接口，所以采用并发方式，而115又做了限流措施，所以需要等待一段时间，并且可能因多次操作而触发限流，建议短时间内不要频繁批量重命名，如触发限流，请到115网页进行验证解除限流后再继续操作。
    </NAlert>
    <div class="flex">
      <NTabs v-model:value="mode" type="card" placement="left" pane-class="w-100!">
        <!-- 查找替换 -->
        <NTabPane name="replace" tab="查找替换">
          <NSpace vertical>
            <NInput
              v-model:value="findText"
              :placeholder="useRegex ? '正则表达式，如 (\\d+)' : '查找内容'"
              :status="regexError ? 'error' : undefined"
              clearable
            />
            <NInput
              v-model:value="replaceText"
              :placeholder="useRegex ? '替换为，可用 $1 $2 引用捕获组' : '替换为'"
              clearable
            />
            <NSpace align="center" :wrap="false">
              <NCheckbox v-model:checked="useRegex"> 正则匹配 </NCheckbox>
              <NButton
                v-if="useRegex"
                text
                tag="a"
                href="https://developer.mozilla.org/zh-CN/docs/Web/JavaScript/Guide/Regular_expressions"
                target="_blank"
                type="primary"
              >
                语法参考
              </NButton>
              <NCheckbox v-if="useRegex" v-model:checked="regexIgnoreCase">忽略大小写</NCheckbox>
              <NCheckbox v-if="useRegex" v-model:checked="regexGlobal">全部替换</NCheckbox>
            </NSpace>
            <NSpace v-if="useRegex" align="center">
              <span class="text-xs text-gray-400">匹配范围</span>
              <NRadioGroup v-model:value="replaceScope" size="small">
                <NRadioButton value="full">全名</NRadioButton>
                <NRadioButton value="name">仅文件名</NRadioButton>
                <NRadioButton value="ext">仅扩展名</NRadioButton>
              </NRadioGroup>
            </NSpace>
            <NText v-if="regexError" type="error" class="text-xs">{{ regexError }}</NText>
          </NSpace>
        </NTabPane>

        <!-- 添加前后缀 -->
        <NTabPane name="affix" tab="添加前后缀">
          <NSpace vertical>
            <NInput v-model:value="prefix" placeholder="添加前缀" clearable />
            <NInput v-model:value="suffix" placeholder="添加后缀（扩展名前）" clearable />
          </NSpace>
        </NTabPane>

        <!-- 序号重命名 -->
        <NTabPane name="sequential" tab="序号重命名">
          <NSpace vertical>
            <NInput v-model:value="seqTemplate" placeholder="命名模板，如：文件_{n}" clearable>
              <template #suffix>
                <NTooltip>
                  <template #trigger>
                    <NIcon size="16" class="cursor-help">
                      <InfoCircleOutlined />
                    </NIcon>
                  </template>
                  <div>
                    <p>{n} - 序号（如：1, 2, 3）</p>
                    <p>{n:3} - 补零序号（如：001, 002）</p>
                    <p>{name} - 原文件名（不含扩展名）</p>
                    <p>{ext} - 原扩展名</p>
                  </div>
                </NTooltip>
              </template>
            </NInput>
            <NInputNumber v-model:value="seqStart" :min="0" placeholder="起始序号">
              <template #prefix>起始序号</template>
            </NInputNumber>
          </NSpace>
        </NTabPane>

        <!-- 大小写转换 -->
        <NTabPane name="case" tab="大小写转换">
          <NRadioGroup v-model:value="caseMode">
            <NSpace vertical>
              <NRadio value="upper">全部大写（ABC）</NRadio>
              <NRadio value="lower">全部小写（abc）</NRadio>
              <NRadio value="capitalize">首字母大写（Abc Def）</NRadio>
              <NRadio value="nameLower">仅文件名小写</NRadio>
              <NRadio value="nameUpper">仅文件名大写</NRadio>
              <NRadio value="extLower">仅扩展名小写</NRadio>
              <NRadio value="extUpper">仅扩展名大写</NRadio>
            </NSpace>
          </NRadioGroup>
        </NTabPane>
      </NTabs>
      <!-- 预览 -->
      <div class="ml-2">
        <NDataTable
          :columns="previewColumns"
          :data="previewList"
          :row-key="(row: PreviewItem) => row.fid"
          flex-height
          class="h-[60vh]"
        />
      </div>
    </div>
    <template #action>
      <NSpace justify="end">
        <NButton @click="show = false">取消</NButton>
        <NButton type="primary" :loading="submitting" :disabled="!hasChanges" @click="handleSubmit">
          确定（{{ changedCount }} 项）
        </NButton>
      </NSpace>
    </template>
  </NModal>
</template>

<script setup lang="tsx">
  import { InfoCircleOutlined } from '@vicons/antd';
  import { updateFile } from '@/api/file';
  import type { MyFile } from '@/api/types/file';
  import {
    sleep,
    isRateLimitError,
    getBackoffDelay,
    MAX_RATE_LIMIT_RETRY,
  } from '@/utils/rateLimit';
  import type { DataTableColumns } from 'naive-ui';

  const show = defineModel('show', {
    type: Boolean,
    default: false,
  });

  const props = defineProps<{
    files: MyFile[];
  }>();

  const emits = defineEmits(['success']);

  const message = useMessage();

  // 模式
  const mode = ref<'replace' | 'affix' | 'sequential' | 'case'>('replace');

  // 查找替换
  const findText = ref('');
  const replaceText = ref('');
  const useRegex = ref(false);
  const regexIgnoreCase = ref(false);
  const regexGlobal = ref(true);
  const replaceScope = ref<'full' | 'name' | 'ext'>('full');

  // 正则校验
  const regexError = computed(() => {
    if (!useRegex.value || !findText.value) return '';
    try {
      new RegExp(findText.value);
      return '';
    } catch (e) {
      return `正则表达式无效: ${(e as Error).message}`;
    }
  });

  // 前后缀
  const prefix = ref('');
  const suffix = ref('');

  // 序号
  const seqTemplate = ref('{name}_{n:3}');
  const seqStart = ref(1);

  // 大小写
  const caseMode = ref<
    'upper' | 'lower' | 'capitalize' | 'nameLower' | 'nameUpper' | 'extLower' | 'extUpper'
  >('upper');

  // 提交状态
  const submitting = ref(false);

  // 重置表单
  watch(show, (val) => {
    if (val) {
      findText.value = '';
      replaceText.value = '';
      useRegex.value = false;
      regexIgnoreCase.value = false;
      regexGlobal.value = true;
      replaceScope.value = 'full';
      prefix.value = '';
      suffix.value = '';
      seqTemplate.value = '{name}_{n:3}';
      seqStart.value = 1;
      caseMode.value = 'upper';
      mode.value = 'replace';
    }
  });

  // 分离文件名和扩展名
  function splitName(fileName: string): [string, string] {
    const lastDot = fileName.lastIndexOf('.');
    if (lastDot <= 0) return [fileName, ''];
    return [fileName.substring(0, lastDot), fileName.substring(lastDot)];
  }

  // 生成新文件名
  function generateNewName(file: MyFile, index: number): string {
    const isFolder = file.fc === '0';
    const fullName = file.fn;

    if (mode.value === 'replace') {
      if (!findText.value) return fullName;
      if (useRegex.value) {
        if (regexError.value) return fullName;
        try {
          let flags = regexIgnoreCase.value ? 'i' : '';
          if (regexGlobal.value) flags += 'g';
          const regex = new RegExp(findText.value, flags);
          const scope = replaceScope.value;
          if (scope === 'full' || isFolder) {
            return fullName.replace(regex, replaceText.value);
          }
          const [name, ext] = splitName(fullName);
          if (scope === 'name') {
            return name.replace(regex, replaceText.value) + ext;
          }
          // scope === 'ext'
          return name + ext.replace(regex, replaceText.value);
        } catch {
          return fullName;
        }
      }
      return fullName.replaceAll(findText.value, replaceText.value);
    }

    if (mode.value === 'affix') {
      if (!prefix.value && !suffix.value) return fullName;
      if (isFolder) {
        return `${prefix.value}${fullName}${suffix.value}`;
      }
      const [name, ext] = splitName(fullName);
      return `${prefix.value}${name}${suffix.value}${ext}`;
    }

    if (mode.value === 'sequential') {
      if (!seqTemplate.value) return fullName;
      const [name, ext] = isFolder ? [fullName, ''] : splitName(fullName);
      const num = seqStart.value + index;
      let result = seqTemplate.value;
      // {n:3} → 补零
      result = result.replace(/\{n:(\d+)\}/g, (_, width) =>
        String(num).padStart(Number(width), '0'),
      );
      // {n} → 普通序号
      result = result.replace(/\{n\}/g, String(num));
      // {name} → 原文件名
      result = result.replace(/\{name\}/g, name);
      // {ext} → 扩展名（含点号）
      result = result.replace(/\{ext\}/g, ext);
      // 如果模板没有 {ext}，自动追加原扩展名
      if (!seqTemplate.value.includes('{ext}') && ext) {
        result += ext;
      }
      return result;
    }

    if (mode.value === 'case') {
      const [name, ext] = isFolder ? [fullName, ''] : splitName(fullName);
      switch (caseMode.value) {
        case 'upper':
          return fullName.toUpperCase();
        case 'lower':
          return fullName.toLowerCase();
        case 'capitalize':
          return name.replace(/\b\w/g, (c) => c.toUpperCase()) + ext;
        case 'nameLower':
          return name.toLowerCase() + ext;
        case 'nameUpper':
          return name.toUpperCase() + ext;
        case 'extLower':
          return name + ext.toLowerCase();
        case 'extUpper':
          return name + ext.toUpperCase();
      }
    }

    return fullName;
  }

  // 差异分段
  interface DiffSegment {
    text: string;
    changed: boolean;
  }

  interface PreviewItem {
    fid: string;
    oldName: string;
    newName: string;
    changed: boolean;
    oldSegments: DiffSegment[];
    newSegments: DiffSegment[];
  }

  function diffSegments(oldStr: string, newStr: string): [DiffSegment[], DiffSegment[]] {
    if (oldStr === newStr) {
      return [[{ text: oldStr, changed: false }], [{ text: newStr, changed: false }]];
    }
    // 找公共前缀
    let prefixLen = 0;
    while (
      prefixLen < oldStr.length &&
      prefixLen < newStr.length &&
      oldStr[prefixLen] === newStr[prefixLen]
    ) {
      prefixLen++;
    }
    // 找公共后缀
    let suffixLen = 0;
    while (
      suffixLen < oldStr.length - prefixLen &&
      suffixLen < newStr.length - prefixLen &&
      oldStr[oldStr.length - 1 - suffixLen] === newStr[newStr.length - 1 - suffixLen]
    ) {
      suffixLen++;
    }
    const oldSegs: DiffSegment[] = [];
    const newSegs: DiffSegment[] = [];
    const oldMid = oldStr.substring(prefixLen, oldStr.length - suffixLen);
    const newMid = newStr.substring(prefixLen, newStr.length - suffixLen);
    const pre = oldStr.substring(0, prefixLen);
    const suf = oldStr.substring(oldStr.length - suffixLen);
    if (pre) {
      oldSegs.push({ text: pre, changed: false });
      newSegs.push({ text: pre, changed: false });
    }
    if (oldMid) oldSegs.push({ text: oldMid, changed: true });
    if (newMid) newSegs.push({ text: newMid, changed: true });
    if (suf) {
      oldSegs.push({ text: suf, changed: false });
      newSegs.push({ text: suf, changed: false });
    }
    return [oldSegs, newSegs];
  }

  // 预览列表
  const previewList = computed(() =>
    props.files.map((file, index) => {
      const newName = generateNewName(file, index);
      const [oldSegments, newSegments] = diffSegments(file.fn, newName);
      return {
        fid: file.fid,
        oldName: file.fn,
        newName,
        changed: newName !== file.fn,
        oldSegments,
        newSegments,
      };
    }),
  );

  const hasChanges = computed(() => previewList.value.some((item) => item.changed));
  const changedCount = computed(() => previewList.value.filter((item) => item.changed).length);

  // 表格列
  const renderDiffSegments = (segments: DiffSegment[], type: 'old' | 'new') =>
    segments.map((seg, i) =>
      seg.changed ? (
        <NEl
          key={i}
          tag="span"
          style={{
            color: `var(--${type === 'old' ? 'error' : 'success'}-color)`,
            backgroundColor: `color-mix(in srgb, var(--${type === 'old' ? 'error' : 'success'}-color) 12%, transparent)`,
            borderRadius: '3px',
            padding: '0 2px',
          }}
        >
          {seg.text}
        </NEl>
      ) : (
        <span key={i}>{seg.text}</span>
      ),
    );

  const previewColumns = computed<DataTableColumns<PreviewItem>>(() => [
    {
      title: '原文件名',
      key: 'oldName',
      ellipsis: { tooltip: true },
      render: (row) => {
        if (mode.value === 'replace' && findText.value) {
          return (
            <NHighlight
              text={row.oldName}
              patterns={[findText.value]}
              autoEscape={!useRegex.value}
            />
          );
        }
        return renderDiffSegments(row.oldSegments, 'old');
      },
    },
    {
      title: '新文件名',
      key: 'newName',
      ellipsis: { tooltip: true },
      render: (row) => renderDiffSegments(row.newSegments, 'new'),
    },
  ]);

  // 提交
  const handleSubmit = async () => {
    const toRename = previewList.value.filter((item) => item.changed);
    if (toRename.length === 0) return;

    // 检查空名称
    const emptyNames = toRename.filter((item) => !item.newName.trim());
    if (emptyNames.length > 0) {
      message.error('存在空的文件名，请检查');
      return;
    }

    submitting.value = true;
    let successCount = 0;
    let failCount = 0;

    try {
      for (const item of toRename) {
        let retries = 0;
        while (retries <= MAX_RATE_LIMIT_RETRY) {
          try {
            await updateFile({
              file_id: item.fid,
              file_name: item.newName,
            });
            successCount++;
            break;
          } catch (error) {
            if (isRateLimitError(error) && retries < MAX_RATE_LIMIT_RETRY) {
              retries++;
              await sleep(getBackoffDelay(retries));
            } else {
              failCount++;
              console.error(`重命名失败: ${item.oldName}`, error);
              break;
            }
          }
        }
        // 请求间隔避免频繁调用
        if (toRename.indexOf(item) < toRename.length - 1) {
          await sleep(200);
        }
      }

      if (failCount === 0) {
        message.success(`批量重命名完成，共 ${successCount} 项`);
      } else {
        message.warning(`批量重命名完成：成功 ${successCount} 项，失败 ${failCount} 项`);
      }

      emits('success');
      show.value = false;
    } finally {
      submitting.value = false;
    }
  };
</script>
