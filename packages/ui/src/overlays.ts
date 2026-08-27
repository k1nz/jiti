import { ref } from 'vue';

/** 错题筛选等弹层打开时，Esc 只关弹层。 */
export const overlayCount = ref(0);

export function pushOverlay() {
  overlayCount.value += 1;
}

export function popOverlay() {
  overlayCount.value = Math.max(0, overlayCount.value - 1);
}

export function hasOpenOverlay() {
  return overlayCount.value > 0;
}
