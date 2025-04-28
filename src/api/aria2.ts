import { useSettingStore } from '@/store/setting';
import { useUserStore } from '@/store/user';
import { aria2Server } from '@/utils/http/alova';

const settingStore = useSettingStore();
const userStore = useUserStore();

export const getVersion = () =>
  aria2Server.Post('/jsonrpc', {
    jsonrpc: '2.0',
    id: 'qwer',
    method: 'aria2.getVersion',
  });

export const addUri = (url: string, name: string) =>
  aria2Server.Post('/jsonrpc', {
    jsonrpc: '2.0',
    id: 'qwer',
    method: 'aria2.addUri',
    params: [
      [url],
      {
        dir: settingStore.downloadSetting.downloadPath,
        out: name,
        header: [
          'User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36',
          `Authorization: Bearer ${userStore.accessToken}`,
        ],
      },
    ],
  });

export const tellStatus = (gid: string) =>
  aria2Server.Post('/jsonrpc', {
    jsonrpc: '2.0',
    id: 'qwer',
    method: 'aria2.tellStatus',
    params: [gid],
  });
