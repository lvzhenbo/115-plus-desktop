// @ts-nocheck
import Hls from 'hls.js';
import { fetch } from '@tauri-apps/plugin-http';

const BaseLoader = Hls.DefaultConfig.loader;
export default class CustomLoader extends BaseLoader {
  loadInternal() {
    const { config, context } = this;
    if (!config || !context) {
      return;
    }
    const url = context.url;
    // if (context.keyInfo) {
    // Do custom request/response
    return new Promise((resolve, reject) => {
      // doing fetch in custom XHR loader for demo purposes:
      fetch(url)
        .then((response) => {
          // 根据响应类型判断使用 text() 还是 arrayBuffer() 方法
          if (context.responseType === 'text' || context.responseType === 'json') {
            resolve(response.text());
          } else {
            resolve(response.arrayBuffer());
          }
        })
        .catch((error) => {
          reject(error);
        });
    })
      .then((responseData) => {
        // Replace the XMLHttpRequest "loader" with an object in the state of a completed request
        this.loader = {
          readyState: 4,
          status: 200,
          statusText: '',
          responseType: context.responseType,
          response: responseData,
          responseText: responseData,
          responseURL: url,
        };
        this.readystatechange();
      })
      .catch((error) => {
        console.error('Custom loader error:', error);

        // invoke error or replace loader with bad status and invole `readystatechange()`
      });
    // }
    // standard request
    // super.loadInternal();
  }
}
