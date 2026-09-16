import type { Ref } from 'vue';

// 把弹窗的开关状态同步到路由 query，让浏览器 / 鼠标侧键的「回退」也能作用在弹窗上：
//
// - 打开弹窗时 push 一个携带标记参数的条目，关闭时则用 replace 就地清理；
// - 回退（back）时 URL 先去掉标记参数，弹窗随之关闭；前进（forward）可以重新打开；
// - 切换页面或点击菜单跳转时 URL 变化，弹窗会一并关闭，不会残留到新的页面上下文。
//
// `enabled` 用于跳过不该参与全局导航的实例（例如弹窗内复用的 FileExplorer）。

export const useModalQuerySync = (
  show: Ref<boolean>,
  key: string,
  enabled: () => boolean = () => true,
) => {
  const route = useRoute();
  const router = useRouter();

  // 浮层状态 → URL：打开时占一个 history 条目，关闭时清理。
  watch(show, (value) => {
    if (!enabled()) return;

    const inUrl = route.query[key] !== undefined;
    if (value && !inUrl) {
      void router.push({ query: { ...route.query, [key]: '1' } });
    } else if (!value && inUrl) {
      const query = { ...route.query };
      delete query[key];
      void router.replace({ query });
    }
  });

  // URL → 浮层状态：响应浏览器前进 / 后退带来的变化。
  watch(
    () => route.query[key],
    (value) => {
      if (!enabled()) return;

      if (value === undefined && show.value) {
        show.value = false;
      } else if (value !== undefined && !show.value) {
        show.value = true;
      }
    },
  );
};
