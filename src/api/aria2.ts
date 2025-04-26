import { aria2Server } from '@/utils/http/alova';

export const getVersion = () =>
  aria2Server.Post('/jsonrpc', {
    jsonrpc: '2.0',
    id: 'qwer',
    method: 'aria2.getVersion',
  });
