import { describe, expect, it } from 'vitest';
import type { PermissionItem, PermissionsSnapshot } from '../src/ipc/bindings';
import { footerPermissionWarning, missingRequired } from '../src/permissions';

function item(granted: boolean): PermissionItem {
  return {
    id: 'accessibility',
    title: '辅助功能',
    description: '读取选中文字',
    granted,
    required: true,
    hint: null,
  };
}

function snap(partial: Partial<PermissionsSnapshot>): PermissionsSnapshot {
  return {
    platform: 'macos',
    items: [],
    allRequiredGranted: true,
    onboardingSeen: false,
    needsOnboarding: false,
    ...partial,
  };
}

describe('permissions helpers', () => {
  it('列出未授予的必需权限', () => {
    const missing = missingRequired(snap({ items: [item(false)], allRequiredGranted: false }));
    expect(missing).toHaveLength(1);
    expect(missing[0].id).toBe('accessibility');
  });

  it('全部授予时不显示页脚告警', () => {
    expect(footerPermissionWarning(snap({ items: [item(true)] }))).toBeNull();
    expect(footerPermissionWarning(null)).toBeNull();
  });

  it('缺失权限时页脚提示权限名', () => {
    expect(
      footerPermissionWarning(snap({ items: [item(false)], allRequiredGranted: false })),
    ).toBe('辅助功能未开启');
  });
});
