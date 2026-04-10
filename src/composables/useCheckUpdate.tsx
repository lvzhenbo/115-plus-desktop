import type { Update } from '@tauri-apps/plugin-updater';
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { useSettingStore } from '@/store/setting';
import { useDownloadManager } from '@/composables/useDownloadManager';
import { useUploadManager } from '@/composables/useUploadManager';
import { getActiveUploads } from '@/db/uploads';
import { ask } from '@tauri-apps/plugin-dialog';
import { filesize } from 'filesize';
import {
  NA,
  NAlert,
  NBlockquote,
  NButton,
  NH3,
  NLi,
  NOl,
  NP,
  NScrollbar,
  NText,
  NUl,
} from 'naive-ui';
import {
  Marked,
  type Token,
  type TokenizerExtension,
  type TokenizerThis,
  type Tokens,
} from 'marked';

const isChecking = ref(false);

type GfmAlertKind = 'note' | 'tip' | 'important' | 'warning' | 'caution';

const GFM_ALERT_META = {
  note: { title: '提示', type: 'info' },
  tip: { title: '建议', type: 'success' },
  important: { title: '重要', type: 'warning' },
  warning: { title: '警告', type: 'warning' },
  caution: { title: '注意', type: 'error' },
} as const;

type GfmAlertType = (typeof GFM_ALERT_META)[GfmAlertKind]['type'];

interface GfmAlertToken extends Tokens.Generic {
  type: 'gfmAlert';
  raw: string;
  alertType: GfmAlertType;
  title: string;
  tokens: Token[];
}

type ReleaseNoteToken = Token | GfmAlertToken;

const GFM_ALERT_START_RE = / {0,3}>[ \t]?\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]/i;
const GFM_ALERT_BLOCK_RE = /^(?: {0,3}>[^\n]*(?:\n|$))+/;
const GFM_ALERT_HEADER_RE = /^\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\][ \t]*(.*)$/i;

function extractGfmAlert(src: string) {
  const raw = src.match(GFM_ALERT_BLOCK_RE)?.[0];
  if (!raw) return null;

  const lines = raw
    .replace(/\n$/, '')
    .split('\n')
    .map((line) => line.replace(/^ {0,3}>[ \t]?/, ''));
  const [headerLine = '', ...bodyLines] = lines;
  const headerMatch = headerLine.match(GFM_ALERT_HEADER_RE);

  if (!headerMatch) return null;

  const [, kindRaw, firstLine = ''] = headerMatch;
  const kind = kindRaw.toLowerCase() as GfmAlertKind;
  const body = (firstLine ? [firstLine, ...bodyLines] : bodyLines).join('\n');

  return { raw, body, kind };
}

const gfmAlertExtension: TokenizerExtension = {
  name: 'gfmAlert',
  level: 'block',
  start(this: TokenizerThis, src: string) {
    return src.match(GFM_ALERT_START_RE)?.index;
  },
  tokenizer(this: TokenizerThis, src: string) {
    const alert = extractGfmAlert(src);
    if (!alert) return;

    const meta = GFM_ALERT_META[alert.kind];
    const token: GfmAlertToken = {
      type: 'gfmAlert',
      raw: alert.raw,
      alertType: meta.type,
      title: meta.title,
      tokens: [],
    };

    this.lexer.blockTokens(alert.body, token.tokens);
    return token;
  },
};

const releaseNotesParser = new Marked({ gfm: true });
releaseNotesParser.use({ extensions: [gfmAlertExtension] });

export const useCheckUpdate = () => {
  const message = useMessage();
  const dialog = useDialog();
  const notification = useNotification();
  const settingStore = useSettingStore();

  function renderInlineTokens(tokens: Token[]): VNode[] {
    return tokens.map((token) => {
      switch (token.type) {
        case 'escape': {
          const escaped = token as Tokens.Escape;
          return <>{escaped.text}</>;
        }
        case 'text': {
          const text = token as Tokens.Text;
          if (text.tokens) return <>{renderInlineTokens(text.tokens)}</>;
          return <>{text.text}</>;
        }
        case 'strong':
          return <NText strong>{renderInlineTokens((token as Tokens.Strong).tokens)}</NText>;
        case 'em':
          return <NText italic>{renderInlineTokens((token as Tokens.Em).tokens)}</NText>;
        case 'del':
          return <NText delete>{renderInlineTokens((token as Tokens.Del).tokens)}</NText>;
        case 'br':
          return <NText>{'\n'}</NText>;
        case 'codespan':
          return <NText code>{(token as Tokens.Codespan).text}</NText>;
        case 'image': {
          const image = token as Tokens.Image;
          return <NText>{image.raw}</NText>;
        }
        case 'link': {
          const link = token as Tokens.Link;
          return (
            <NA {...{ href: link.href, target: '_blank' }}>{renderInlineTokens(link.tokens)}</NA>
          );
        }
        default:
          return <>{(token as Tokens.Generic).raw}</>;
      }
    });
  }

  function renderTokens(tokens: ReleaseNoteToken[]): VNode[] {
    return tokens.map((token) => {
      switch (token.type) {
        case 'space':
          return <></>;
        case 'text': {
          const text = token as Tokens.Text;
          if (text.tokens) return <>{renderInlineTokens(text.tokens)}</>;
          return <>{text.text}</>;
        }
        case 'gfmAlert': {
          const alert = token as GfmAlertToken;
          return (
            <NAlert class="my-3" type={alert.alertType} title={alert.title}>
              {renderTokens(alert.tokens as ReleaseNoteToken[])}
            </NAlert>
          );
        }
        case 'heading': {
          const heading = token as Tokens.Heading;
          return <NH3>{renderInlineTokens(heading.tokens)}</NH3>;
        }
        case 'hr':
          return <NP class="whitespace-pre-wrap">{(token as Tokens.Hr).raw}</NP>;
        case 'code': {
          const code = token as Tokens.Code;
          return <NP class="whitespace-pre-wrap">{code.raw}</NP>;
        }
        case 'blockquote': {
          const quote = token as Tokens.Blockquote;
          return <NBlockquote>{renderTokens(quote.tokens as ReleaseNoteToken[])}</NBlockquote>;
        }
        case 'paragraph': {
          const para = token as Tokens.Paragraph;
          return <NP class="whitespace-pre-wrap">{renderInlineTokens(para.tokens)}</NP>;
        }
        case 'list': {
          const list = token as Tokens.List;
          if (list.ordered) {
            return (
              <NOl>
                {list.items.map((item, index) => (
                  <NLi key={index}>{renderTokens(item.tokens as ReleaseNoteToken[])}</NLi>
                ))}
              </NOl>
            );
          }

          return (
            <NUl>
              {list.items.map((item, index) => (
                <NLi key={index}>{renderTokens(item.tokens as ReleaseNoteToken[])}</NLi>
              ))}
            </NUl>
          );
        }
        case 'table': {
          return <NP class="whitespace-pre-wrap">{(token as Tokens.Table).raw}</NP>;
        }
        default:
          return <>{(token as Tokens.Generic).raw}</>;
      }
    });
  }

  function renderBody(body: string | undefined) {
    if (!body) return () => '暂无更新说明';
    const tokens = releaseNotesParser.lexer(body) as unknown as ReleaseNoteToken[];
    return () => <NScrollbar class="max-h-[60vh]">{renderTokens(tokens)}</NScrollbar>;
  }

  async function confirmAndInstall(update: Update) {
    const confirmed = await new Promise<boolean>((resolve) => {
      dialog.info({
        title: `发现新版本 v${update.version}`,
        content: renderBody(update.body),
        positiveText: '立即更新',
        negativeText: '稍后再说',
        onPositiveClick: () => resolve(true),
        onNegativeClick: () => resolve(false),
        onClose: () => resolve(false),
        onMaskClick: () => resolve(false),
      });
    });

    if (!confirmed) return;

    const msgReactive = message.loading('正在下载更新...', { duration: 0 });

    await update.download((event) => {
      switch (event.event) {
        case 'Started':
          msgReactive.content = `正在下载更新 (${filesize(event.data.contentLength ?? 0)})...`;
          break;
        case 'Finished':
          msgReactive.destroy();
          message.success('下载完成，即将安装更新...');
          break;
      }
    });

    // 检查是否有活跃任务，有则弹出确认
    const { hasActiveDownloads } = useDownloadManager();
    const uploads = await getActiveUploads();
    const hasActive = hasActiveDownloads.value || uploads.length > 0;

    if (hasActive) {
      const userConfirmed = await ask(
        '当前有正在进行的传输任务，更新将暂停所有任务并重启应用，确定继续？',
        {
          title: '提示',
          kind: 'warning',
          okLabel: '确定',
          cancelLabel: '取消',
        },
      );
      if (!userConfirmed) return;
    }

    // 暂停所有任务，避免进程占用导致 Windows 安装失败
    const { pauseAllTasks: pauseAllDownloads } = useDownloadManager();
    const { pauseAllTasks: pauseAllUploads } = useUploadManager();
    await Promise.all([pauseAllDownloads(), pauseAllUploads()]);
    await update.install();
    await relaunch();
  }

  async function checkForUpdate(options?: { silent?: boolean }) {
    const silent = options?.silent ?? false;
    const proxy = settingStore.generalSetting.updateProxy || undefined;

    if (isChecking.value) return;
    isChecking.value = true;

    try {
      const update = await check({ proxy });

      if (!update) {
        if (!silent) {
          message.success('当前已是最新版本');
        }
        return;
      }

      if (silent) {
        const n = notification.info({
          title: '发现新版本！',
          description: `v${update.version}`,
          action: () => (
            <NButton
              type="primary"
              text
              onClick={() => {
                n.destroy();
                confirmAndInstall(update);
              }}
            >
              查看详情
            </NButton>
          ),
        });
        return;
      }

      await confirmAndInstall(update);
    } catch (error) {
      console.error(error);

      if (!silent) {
        message.error(`检查更新失败: ${error instanceof Error ? error.message : String(error)}`);
      }
    } finally {
      isChecking.value = false;
    }
  }

  return { isChecking, checkForUpdate };
};
