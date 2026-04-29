export function normalizeFontFamily(fontFamily: string) {
  return fontFamily
    .trim()
    .replace(/^@/, '')
    .replace(/^['"]+|['"]+$/g, '')
    .replace(/\s+/g, ' ')
    .trim();
}
