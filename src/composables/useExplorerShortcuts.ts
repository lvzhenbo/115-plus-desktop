// 文件列表快捷键的「实例所有权」注册表。
//
// FileExplorer 可能同时存在多个实例（例如 FolderModal 弹窗中复用的列表），
// 快捷键只应由最上层的实例响应。注册表必须定义在组件之外才能跨实例共享
// （`<script setup>` 顶层的代码会随每个实例执行，无法用来保存共享状态），
// 这里用 vueuse 的 `createGlobalState` 维护一份全局单例。

export const useExplorerShortcuts = createGlobalState(() => {
  const owners: symbol[] = [];

  return {
    /** 登记一个实例，后登记的排在栈顶。 */
    register: (token: symbol) => {
      owners.push(token);
    },

    /** 移除一个实例。 */
    unregister: (token: symbol) => {
      const index = owners.indexOf(token);
      if (index >= 0) owners.splice(index, 1);
    },

    /** 该实例是否位于栈顶（最上层、应响应快捷键的实例）。 */
    isTop: (token: symbol) => owners[owners.length - 1] === token,
  };
});
