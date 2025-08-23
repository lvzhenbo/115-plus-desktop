import type { UserInfoResponseData } from '@/api/types/user';
import store from '.';
import router from '@/router';

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

    const logout = () => {
      accessToken.value = '';
      refreshToken.value = '';
      expiresIn.value = 0;
      userInfo.value = null;
      latestCopyFolder.value = '0';
      latestMoveFolder.value = '0';
      latestSaveFolder.value = '0';
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
      logout,
      setLatestFolder,
      getLatestFolder,
    };
  },
  {
    persist: true,
  },
);

export function useUserStoreWithOut() {
  return useUserStore(store);
}
