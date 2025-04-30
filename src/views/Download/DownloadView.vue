<template>
  <div class="px-6 py-3">
    <NDataTable
      ref="tableRef"
      remote
      flex-height
      :columns
      :data="settingStore.downloadSetting.downloadList"
      :row-key="(row: DownLoadFile) => row.gid"
      class="h-[calc(100vh-133px)]"
    />
  </div>
</template>

<script setup lang="tsx">
  import { useSettingStore, type DownLoadFile } from '@/store/setting';
  import { filesize } from 'filesize';
  import { NProgress, NText, type DataTableColumns } from 'naive-ui';

  const settingStore = useSettingStore();
  const columns: DataTableColumns<DownLoadFile> = [
    {
      title: '文件名',
      key: 'name',
      ellipsis: {
        tooltip: true,
      },
    },
    {
      title: '大小',
      key: 'size',
      width: 100,
      render(row) {
        return row.size ? filesize(row.size, { standard: 'jedec' }) : '';
      },
    },
    {
      title: '进度',
      key: 'percentDone',
      width: 300,
      render(row) {
        if (row.status === 'error') {
          return <NText type="error">下载失败</NText>;
        } else if (row.status === 'waiting') {
          return <NText type="warning">等待中</NText>;
        } else if (row.status === 'active') {
          return <NProgress type="line" percentage={row.progress} processing />;
        } else if (row.status === 'paused') {
          return <NText type="info">已暂停</NText>;
        } else if (row.status === 'complete') {
          return <NText type="success">下载完成</NText>;
        }
      },
    },
    {
      title: '操作',
      key: 'action',
      width: 110,
      // render: (row) => {
      //   return (
      //     <NSpace>
      //       {row.file_id ? (
      //         <NButton
      //           text
      //           onClick={() =>
      //             router.push({
      //               name: 'Home',
      //               query: {
      //                 fid: row.file_id,
      //               },
      //             })
      //           }
      //         >
      //           {{
      //             icon: () => (
      //               <NIcon>
      //                 <FolderOutlined />
      //               </NIcon>
      //             ),
      //           }}
      //         </NButton>
      //       ) : null}
      //       <NButton
      //         text
      //         onClick={async () => {
      //           await copy(row.url);
      //           message.success('复制成功！');
      //         }}
      //       >
      //         {{
      //           icon: () => (
      //             <NIcon>
      //               <CopyOutlined />
      //             </NIcon>
      //           ),
      //         }}
      //       </NButton>
      //       <NButton
      //         text
      //         type="error"
      //         onClick={() => {
      //           flag.value = settingStore.cloudDownloadSetting.deleteSourceFile ? 1 : 0;
      //           dialog.warning({
      //             title: '是否确认删除该下载任务？',
      //             content: () => (
      //               <NCheckbox v-model:checked={flag.value} checked-value={1} unchecked-value={0}>
      //                 删除源文件
      //               </NCheckbox>
      //             ),
      //             positiveText: '确定',
      //             negativeText: '取消',
      //             onPositiveClick: () => {
      //               handleDelete(row.info_hash);
      //             },
      //           });
      //         }}
      //       >
      //         {{
      //           icon: () => (
      //             <NIcon>
      //               <DeleteOutlined />
      //             </NIcon>
      //           ),
      //         }}
      //       </NButton>
      //     </NSpace>
      //   );
      // },
    },
  ];
</script>

<style scoped></style>
