import { createDiscreteApi, type ConfigProviderProps, darkTheme, lightTheme } from 'naive-ui';

const themeRef = useOsTheme();
const configProviderPropsRef = computed<ConfigProviderProps>(() => ({
  theme: themeRef.value === 'light' ? lightTheme : darkTheme,
}));

const { message, notification, dialog, loadingBar } = createDiscreteApi(
  ['message', 'dialog', 'notification', 'loadingBar'],
  {
    configProviderProps: configProviderPropsRef,
  },
);

export function useMessage() {
  return message;
}

export function useNotification() {
  return notification;
}

export function useDialog() {
  return dialog;
}

export function useLoadingBar() {
  return loadingBar;
}
