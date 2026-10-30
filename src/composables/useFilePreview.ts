import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { emit, listen } from '@tauri-apps/api/event';
import {
  createMediaList,
  resolvePreviewFile,
  type MediaSource,
  type PreviewFile,
  type VideoPlayback,
} from '@/utils/media';

/** 主窗口共用打开逻辑，搜索和文件列表使用同一个播放器、图片预览。 */
export const useFilePreview = createSharedComposable(() => {
  const message = useMessage();
  const imageShow = ref(false);
  const imageIndex = ref(0);
  const imageFiles = ref<PreviewFile[]>([]);
  // 未访问的搜索图片先保留有效占位地址，受控索引会在补齐原图地址后才切换。
  const imageUrls = computed(() =>
    imageFiles.value.map((file) => file.uo || imageFiles.value[imageIndex.value]?.uo || ''),
  );
  let imageList: ReturnType<typeof createMediaList> | undefined;
  let imageBusy = false;
  let imageSession = 0;
  let playback: VideoPlayback | undefined;
  const unlisten = listen('get-video-list', () => {
    if (playback) void emit('set-video-list', playback);
  });

  watch(imageShow, (show) => {
    if (!show) {
      imageSession++;
      imageList?.cancel();
    }
  });

  async function stepImage(step: number) {
    if (imageBusy || !imageList) return;
    const list = imageList;
    const session = imageSession;
    imageBusy = true;
    try {
      const next = imageIndex.value + step;
      while (next >= list.items.length && list.hasMore()) await list.loadMore();
      if (session !== imageSession) return;
      if (next < 0 || next >= list.items.length) {
        message.info(next < 0 ? '已经是第一张图片了' : '已经是最后一张图片了');
        return;
      }
      const file = await resolvePreviewFile(list.items[next]!);
      if (session !== imageSession) return;
      if (!file.uo) {
        list.items.splice(next, 1);
        if (next < imageIndex.value) imageIndex.value--;
        imageFiles.value = [...list.items];
        message.info('暂不支持预览，请下载后打开');
        return;
      }
      list.items[next] = file;
      imageFiles.value = [...list.items];
      imageIndex.value = next;
    } catch (error) {
      if (session === imageSession)
        message.error(error instanceof Error ? error.message : '图片加载失败');
    } finally {
      if (session === imageSession) imageBusy = false;
    }
  }

  async function openFile(input: PreviewFile, source: MediaSource): Promise<boolean> {
    const file = await resolvePreviewFile(input);
    if (file.isv) {
      playback = { file, source };
      const existingWindow = await WebviewWindow.getByLabel('video-player');
      if (existingWindow) {
        await emit('set-video-list', playback);
        await existingWindow.setFocus();
      } else {
        const window = new WebviewWindow('video-player', {
          url: '/videoPlayer',
          title: file.fn,
          width: 1280,
          height: 720,
          minWidth: 1280,
          minHeight: 720,
          center: true,
          visible: false,
        });
        void window.once('tauri://error', () => message.error('视频窗口创建失败'));
      }
      return true;
    }
    if (!file.uo) {
      message.info('暂不支持预览，请下载后打开');
      return false;
    }

    imageList?.cancel();
    const session = ++imageSession;
    const list = createMediaList(source, 2);
    imageList = list;
    imageBusy = true;
    imageFiles.value = [file];
    imageIndex.value = 0;
    imageShow.value = true;
    void list
      .locate(file)
      .then((index) => {
        if (session !== imageSession || index < 0) return;
        imageFiles.value = [...list.items];
        imageIndex.value = index;
      })
      .catch((error) => {
        if (session === imageSession)
          message.error(error instanceof Error ? error.message : '图片加载失败');
      })
      .finally(() => {
        if (session === imageSession) imageBusy = false;
      });
    return true;
  }

  onScopeDispose(() => {
    imageSession++;
    imageList?.cancel();
    void unlisten.then((stop) => stop());
  });

  return { openFile, imageShow, imageIndex, imageUrls, stepImage };
});
