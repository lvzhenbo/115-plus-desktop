import type { UserInfoResponseData } from '@/api/types/user';
import type { SortConfig, ViewMode } from '@/components/FileExplorer/types';
import store from '.';
import router from '@/router';

export interface FavoriteFolder {
  cid: string;
  name: string;
  pid: string;
  favoritedAt: number;
}

export const useUserStore = defineStore(
  'user',
  () => {
    const accessToken = ref('');
    const refreshToken = ref('');
    const expiresIn = ref(0);
    const userInfo = ref<UserInfoResponseData | null>(null);
    const latestCopyFolder = ref('0');
    const latestMoveFolder = ref('0');
    const latestSaveFolder = ref('0');

    // FileExplorer 视图设置
    const homeViewMode = ref<ViewMode>('list');
    const homeSortConfig = ref<SortConfig>({ field: 'user_utime', direction: 'desc' });
    const folderModalViewMode = ref<ViewMode>('list');
    const folderModalSortConfig = ref<SortConfig>({ field: 'user_utime', direction: 'desc' });

    const favorites = ref<FavoriteFolder[]>([]);

    function addFavorite(cid: string, name: string, pid: string) {
      const existing = favorites.value.find((f) => f.cid === cid);
      if (!existing) {
        favorites.value.push({ cid, name, pid, favoritedAt: Date.now() });
      } else {
        existing.name = name;
        existing.pid = pid;
        existing.favoritedAt = Date.now();
      }
    }

    function removeFavorite(cid: string) {
      favorites.value = favorites.value.filter((f) => f.cid !== cid);
    }

    function isFavorited(cid: string): boolean {
      return favorites.value.some((f) => f.cid === cid);
    }

    const logout = () => {
      accessToken.value = '';
      refreshToken.value = '';
      expiresIn.value = 0;
      userInfo.value = null;
      latestCopyFolder.value = '0';
      latestMoveFolder.value = '0';
      latestSaveFolder.value = '0';
      favorites.value = [];
      router.replace({
        name: 'Login',
      });
    };

    const setLatestFolder = (type: 'copy' | 'move' | 'save', folderId: string) => {
      if (type === 'copy') {
        latestCopyFolder.value = folderId;
      } else if (type === 'move') {
        latestMoveFolder.value = folderId;
      } else if (type === 'save') {
        latestSaveFolder.value = folderId;
      }
    };
    const getLatestFolder = (type: 'copy' | 'move' | 'save') => {
      if (type === 'copy') {
        return latestCopyFolder.value;
      } else if (type === 'move') {
        return latestMoveFolder.value;
      } else if (type === 'save') {
        return latestSaveFolder.value;
      }
    };

    return {
      accessToken,
      refreshToken,
      expiresIn,
      userInfo,
      latestCopyFolder,
      latestMoveFolder,
      latestSaveFolder,
      homeViewMode,
      homeSortConfig,
      folderModalViewMode,
      folderModalSortConfig,
      favorites,
      addFavorite,
      removeFavorite,
      isFavorited,
      logout,
      setLatestFolder,
      getLatestFolder,
    };
  },
  {
    tauri: {
      saveOnChange: true,
    },
  },
);

export function useUserStoreWithOut() {
  return useUserStore(store);
}
