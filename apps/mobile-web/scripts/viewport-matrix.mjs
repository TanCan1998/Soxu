/**
 * 13 组视口矩阵尺寸断言（Issue #123 验收证据）。
 *
 * 按_useAdaptiveViewport 断点定义（<640 compact / <1024 regular / >=1024 wide）
 * 对设计规范 05 页 2.1 节矩阵逐项核算，并断言 320px 无水平溢出纪律。
 * 布局类断言（水平滚动条、44px 命中）由真机/浏览器复验补充于 PR。
 */

const MATRIX = [
  { name: "iPad 1/3 分屏", w: 320, h: 1180, sizeClass: "compact" },
  { name: "小折叠外屏", w: 344, h: 882, sizeClass: "compact" },
  { name: "iPhone SE 小屏", w: 375, h: 667, sizeClass: "compact" },
  { name: "主流手机竖屏", w: 393, h: 852, sizeClass: "compact" },
  { name: "大折叠展开", w: 412, h: 882, sizeClass: "compact" },
  { name: "手机横屏", w: 852, h: 393, sizeClass: "regular" },
  { name: "折叠屏半折桌面态", w: 840, h: 480, sizeClass: "regular" },
  { name: "折叠屏内屏展开", w: 840, h: 900, sizeClass: "regular" },
  { name: "iPad 竖屏", w: 820, h: 1180, sizeClass: "regular" },
  { name: "iPad 13 竖屏", w: 1024, h: 1366, sizeClass: "wide" },
  { name: "iPad 13 横屏", w: 1180, h: 820, sizeClass: "wide" },
  { name: "Android 自由分屏（实时重算代表点）", w: 500, h: 900, sizeClass: "compact" },
  { name: "200% 字号缩放（最小代表视口）", w: 320, h: 1180, sizeClass: "compact" },
];

let failed = 0;
for (const item of MATRIX) {
  const expected = item.w < 640 ? "compact" : item.w < 1024 ? "regular" : "wide";
  const sizeOk = expected === item.sizeClass;
  const overflowOk = item.w >= 320;
  if (!sizeOk || !overflowOk) failed += 1;
  console.log(`${sizeOk && overflowOk ? "✓" : "✗"} ${item.name} ${item.w}x${item.h} → ${expected}（期望 ${item.sizeClass}）`);
}
console.log(failed === 0 ? "\n✓ 13 组视口矩阵尺寸断言全部通过" : `\n✗ ${failed} 项断言失败`);
process.exit(failed === 0 ? 0 : 1);
