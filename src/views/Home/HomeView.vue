<template>
  <div class="p-4">
    <NBreadcrumb class="mb-1">
      <NBreadcrumbItem v-for="item in path" :key="item.cid" @click="handleToFolder(item.cid)">
        <NEllipsis
          class="max-w-60!"
          :tooltip="{
            placement: 'top',
            width: 'trigger',
          }"
        >
          {{ item.name }}
        </NEllipsis>
      </NBreadcrumbItem>
    </NBreadcrumb>
    <NDataTable
      ref="tableRef"
      remote
      flex-height
      :columns
      :data
      :pagination
      :row-key="(row: MyFile) => row.fid"
      v-model:checked-row-keys="checkedRowKeys"
      :loading
      :row-props
      class="h-[calc(100vh-90px)]"
      @update:page="handlePageChange"
    />
  </div>
</template>

<script setup lang="tsx">
  import { type DataTableInst, type DataTableColumns, type PaginationProps } from 'naive-ui';
  import { filesize } from 'filesize';
  import { fileList } from '@/api/file';
  import type { FileListRequestParams, MyFile, Path } from '@/api/types/file';
  import { format } from 'date-fns';
  import type { HTMLAttributes } from 'vue';

  const tableRef = ref<DataTableInst | null>(null);
  const columns: DataTableColumns<MyFile> = [
    {
      type: 'selection',
    },
    {
      title: '文件名',
      key: 'fn',
      ellipsis: {
        tooltip: true,
      },
    },
    {
      title: '大小',
      key: 'fs',
      width: 100,
      render(row) {
        return row.fs ? filesize(row.fs, { standard: 'jedec' }) : '';
      },
    },
    {
      title: '种类',
      key: 'fc',
      width: 100,
      render(row) {
        if (row.fc === '0') {
          return '文件夹';
        } else if (row.fc === '1') {
          return `${row.ico}文件`;
        }
      },
    },
    {
      title: '创建时间',
      key: 'uppt',
      width: 170,
      render(row) {
        return row.uppt ? format(new Date(row.uppt * 1000), 'yyyy-MM-dd HH:mm:ss') : '';
      },
    },
    {
      title: '修改时间',
      key: 'uet',
      width: 170,
      render(row) {
        return row.uet ? format(new Date(row.uet * 1000), 'yyyy-MM-dd HH:mm:ss') : '';
      },
    },
    // {
    //   title: '操作',
    //   key: 'action',
    //   width: 150,
    //   render: (row) => {
    //     return (
    //       <NSpace>
    //         {row.file_id ? (
    //           <NButton
    //             text
    //             onClick={() =>
    //               GM_openInTab(`https://115.com/?cid=${row.file_id}&offset=0&tab=&mode=wangpan`, {
    //                 setParent: settings?.openNewTab.setParent,
    //               })
    //             }
    //           >
    //             {{
    //               icon: () => (
    //                 <NIcon>
    //                   <FolderOutlined />
    //                 </NIcon>
    //               ),
    //             }}
    //           </NButton>
    //         ) : null}
    //         <NButton
    //           text
    //           onClick={async () => {
    //             await copy(row.url);
    //             message.success('复制成功！');
    //           }}
    //         >
    //           {{
    //             icon: () => (
    //               <NIcon>
    //                 <CopyOutlined />
    //               </NIcon>
    //             ),
    //           }}
    //         </NButton>
    //         <NButton
    //           text
    //           onClick={() => {
    //             dialog.warning({
    //               title: '信息提示',
    //               content: () => (
    //                 <div
    //                   style={{
    //                     display: 'flex',
    //                     flexDirection: 'column',
    //                     alignItems: 'center',
    //                   }}
    //                 >
    //                   <div
    //                     style={{
    //                       marginBottom: '10px',
    //                     }}
    //                   >
    //                     是否确认删除该下载任务？
    //                   </div>
    //                   <NCheckbox v-model:checked={flag.value} checked-value={1} unchecked-value={0}>
    //                     删除源文件
    //                   </NCheckbox>
    //                 </div>
    //               ),
    //               positiveText: '确定',
    //               negativeText: '取消',
    //               onPositiveClick: () => {
    //                 handleDelete(row.info_hash);
    //               },
    //             });
    //           }}
    //         >
    //           {{
    //             icon: () => (
    //               <NIcon>
    //                 <DeleteOutlined />
    //               </NIcon>
    //             ),
    //           }}
    //         </NButton>
    //       </NSpace>
    //     );
    //   },
    // },
  ];
  const pagination = reactive<PaginationProps>({
    page: 1,
    itemCount: 0,
    pageSize: 50,
  });
  const loading = ref(false);
  const data = ref<MyFile[]>([]);
  const checkedRowKeys = ref<string[]>([]);
  const params = reactive<FileListRequestParams>({
    cid: '0',
    show_dir: 1,
    offset: 0,
    limit: pagination.pageSize,
  });
  const path = ref<Path[]>([]);
  const forderTemp = ref(new Map<string, number>());

  onMounted(async () => {
    getFileList();
  });

  const getFileList = async () => {
    if (params.cid) forderTemp.value.set(params.cid, pagination.page!);
    params.offset = (pagination.page! - 1) * pagination.pageSize!;
    loading.value = true;
    const res = await fileList({
      ...params,
    });
    data.value = res.data;
    pagination.itemCount = res.count;
    path.value = res.path;
    loading.value = false;
  };

  const rowProps = (row: MyFile): HTMLAttributes => {
    return {
      onDblclick: () => {
        if (row.fc === '0') {
          params.cid = row.fid;
          pagination.page = forderTemp.value.get(row.fid) || 1;
          getFileList();
        }
      },
    };
  };

  const handleToFolder = (cid: string) => {
    params.cid = cid.toString();
    pagination.page = forderTemp.value.get(cid) || 1;
    getFileList();
  };

  const handlePageChange = (page: number) => {
    pagination.page = page;
    getFileList();
  };
</script>

<style scoped></style>
