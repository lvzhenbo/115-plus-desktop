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

    const logout = () => {
      accessToken.value = '';
      refreshToken.value = '';
      expiresIn.value = 0;
      userInfo.value = null;
      router.replace({
        name: 'Login',
      });
    };

    return {
      accessToken,
      refreshToken,
      expiresIn,
      userInfo,
      logout,
    };
  },
  {
    persist: true,
  },
);

export function useUserStoreWithOut() {
  return useUserStore(store);
}
