/**
 * 生成多层 text-shadow 实现字幕描边效果
 * 使用 8 方向 + 4 对角方向的 shadow 模拟描边，比 -webkit-text-stroke 效果更好
 */
export function generateTextShadow(color: string, width: number): string {
  if (width <= 0) return 'none';
  const w = width;
  // 8 方向描边
  const shadows = [
    `${w}px 0 0 ${color}`,
    `${-w}px 0 0 ${color}`,
    `0 ${w}px 0 ${color}`,
    `0 ${-w}px 0 ${color}`,
    `${w}px ${w}px 0 ${color}`,
    `${-w}px ${-w}px 0 ${color}`,
    `${w}px ${-w}px 0 ${color}`,
    `${-w}px ${w}px 0 ${color}`,
  ];
  return shadows.join(', ');
}
