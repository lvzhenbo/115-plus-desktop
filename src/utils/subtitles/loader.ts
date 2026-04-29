import type { SubtitleItem } from '@/api/types/video';
import { fetch as tauriFetch } from '@tauri-apps/plugin-http';

export async function loadSubtitleContent(
  subtitle: SubtitleItem,
  signal?: AbortSignal,
): Promise<string> {
  const response = await tauriFetch(subtitle.url, { signal });
  return response.text();
}
