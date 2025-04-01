import type { UserInfoResponseData } from '@/api/types/user';
import store from '.';

export const useUserStore = defineStore(
  'user',
  () => {
    const accessToken = ref('');
    const refreshToken = ref('');
    const expiresIn = ref(0);
    const userInfo = ref<UserInfoResponseData | null>(null);

    const clearToken = () => {
      accessToken.value = '';
      refreshToken.value = '';
      expiresIn.value = 0;
    };
    return {
      accessToken,
      refreshToken,
      expiresIn,
      userInfo,
      clearToken,
    };
  },
  {
    persist: true,
  },
);

export function useUserStoreWithOut() {
  return useUserStore(store);
}
