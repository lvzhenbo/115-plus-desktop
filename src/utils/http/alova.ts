import { createAlova } from 'alova';
import adapterFetch from './tauriHttpAdapter';
import { useMessage } from '@/composables/useDiscreteApi';

export interface ResponseData<T> {
  state: number;
  code: number;
  message: string;
  data: T;
  error: string;
  errno: number;
}

const message = useMessage();

export const alovaInst = createAlova({
  requestAdapter: adapterFetch(),
  timeout: 40000,
  beforeRequest(method) {
    console.log(method);
  },
  responded: {
    // 请求成功的拦截器
    // 当使用 `alova/fetch` 请求适配器时，第一个参数接收Response对象
    // 第二个参数为当前请求的method实例，你可以用它同步请求前后的配置信息
    onSuccess: async (response, _method) => {
      if (response.status >= 400) {
        throw new Error(response.statusText);
      }
      const json: ResponseData<unknown> = await response.json();
      console.log(json);

      if (!json.state) {
        // 抛出错误或返回reject状态的Promise实例时，此请求将抛出错误
        if (json.code === 40199002) {
          message.error('二维码已失效，请重新扫码');
        } else {
          message.error(json.message);
        }
        throw json;
      }

      // 解析的响应数据将传给method实例的transform钩子函数，这些函数将在后续讲解
      return json;
    },

    // 请求失败的拦截器
    // 请求错误时将会进入该拦截器。
    // 第二个参数为当前请求的method实例，你可以用它同步请求前后的配置信息
    onError: (err, _method) => {
      message.error(JSON.stringify(err));
    },

    // 请求完成的拦截器
    // 当你需要在请求不论是成功、失败、还是命中缓存都需要执行的逻辑时，可以在创建alova实例时指定全局的`onComplete`拦截器，例如关闭请求 loading 状态。
    // 接收当前请求的method实例
    onComplete: async (_method) => {
      // 处理请求完成逻辑
    },
  },
});
