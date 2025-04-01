<template>
  <NLayout content-class="min-h-screen">
    <div class="flex justify-center items-center pt-44">
      <NCard hoverable class="w-100!">
        <div class="flex flex-col items-center">
          <div class="w-55 h-55 relative">
            <NQrCode
              v-if="qrCodeData.qrcode"
              :value="qrCodeData.qrcode"
              class="box-content"
              :size="196"
              error-correction-level="H"
            />
            <div
              v-if="isQrCodeExpired"
              class="absolute top-0 w-full h-full flex flex-col justify-center items-center backdrop-blur-sm font-bold text-xl cursor-pointer"
              :style="{
                color: themeVars.errorColor,
              }"
              @click="init"
            >
              <div>二维码失效</div>
              <div>请点击刷新</div>
            </div>
            <div
              v-if="isScanSuccess"
              class="absolute top-0 w-full h-full flex flex-col justify-center items-center backdrop-blur-sm font-bold text-xl cursor-pointer"
              :style="{
                color: themeVars.successColor,
              }"
            >
              <div>扫描成功</div>
              <div>请在手机点确认以登录</div>
            </div>
            <div
              v-if="isQrCodeExpired"
              class="absolute top-0 w-full h-full flex flex-col justify-center items-center backdrop-blur-sm font-bold text-xl cursor-pointer"
              :style="{
                color: themeVars.errorColor,
              }"
              @click="init"
            >
              <div>已取消登录</div>
              <div>请点击刷新</div>
            </div>
          </div>

          <div class="mt-2">使用 115APP 扫一扫登录</div>
        </div>
      </NCard>
    </div>
  </NLayout>
</template>

<script setup lang="ts">
  import type { AuthDeviceCodeResponseData } from '@/api/types/user';
  import { authDeviceCode, deviceCodeToToken, qrCodeStatus, userInfo } from '@/api/user';
  import { useUserStore } from '@/store/user';

  const themeVars = useThemeVars();
  const codeVerifier = ref('');
  const codeChallenge = ref('');
  const clientId = import.meta.env.VITE_APP_ID;
  const qrCodeData = ref<AuthDeviceCodeResponseData>({
    qrcode: '',
    uid: '',
    sign: '',
    time: 0,
  });
  const isQrCodeExpired = ref(false);
  const isScanSuccess = ref(false);
  const isCencel = ref(false);
  const userStore = useUserStore();
  const message = useMessage();
  const router = useRouter();

  onMounted(async () => {
    init();
  });

  const init = async () => {
    await getCodeChallenge();
    await getQrCodeData();
    getQrCodeStatus();
  };

  const getCodeChallenge = async () => {
    codeVerifier.value = generateRandomString(Math.floor(Math.random() * (128 - 43 + 1)) + 43);
    codeChallenge.value = await generateCodeChallenge();
  };

  const getQrCodeData = async () => {
    const res = await authDeviceCode({
      client_id: clientId,
      code_challenge: codeChallenge.value,
      code_challenge_method: 'sha256',
    });
    qrCodeData.value = res.data;
    isQrCodeExpired.value = false;
    isScanSuccess.value = false;
  };

  const getQrCodeStatus = async () => {
    try {
      const res = await qrCodeStatus({
        uid: qrCodeData.value.uid,
        sign: qrCodeData.value.sign,
        time: qrCodeData.value.time,
      });
      if (!res.data.status) {
        getQrCodeStatus();
      } else if (res.data.status === 1) {
        isScanSuccess.value = true;
        getQrCodeStatus();
      } else if (res.data.status === 2) {
        await getToken();
        await getUserInfo();
      } else {
        isCencel.value = true;
      }
    } catch (error: any) {
      if (error.code === 40199002) {
        isScanSuccess.value = false;
        isQrCodeExpired.value = true;
      }
    }
  };

  const getToken = async () => {
    await deviceCodeToToken({
      uid: qrCodeData.value.uid,
      code_verifier: codeVerifier.value,
    });
  };

  const getUserInfo = async () => {
    try {
      const res = await userInfo();
      userStore.userInfo = res.data;
      message.success('登录成功！');
      router.replace({ name: 'Home' });
    } catch (_error) {}
  };

  const generateRandomString = (length: number) => {
    const characters = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~';
    let result = '';
    const charactersLength = characters.length;
    for (let i = 0; i < length; i++) {
      result += characters.charAt(Math.floor(Math.random() * charactersLength));
    }
    return result;
  };

  const sha256 = async (plain: string) => {
    const encoder = new TextEncoder();
    const data = encoder.encode(plain);
    const hash = await crypto.subtle.digest('SHA-256', data);
    return hash;
  };

  const base64urlencode = (buffer: ArrayBuffer) => {
    const base64String = btoa(String.fromCharCode(...new Uint8Array(buffer)));
    return base64String;
  };

  const generateCodeChallenge = async () => {
    const hashed = await sha256(codeVerifier.value);
    return base64urlencode(hashed);
  };
</script>

<style scoped></style>
