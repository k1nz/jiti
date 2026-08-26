import type { PermissionItem, PermissionsSnapshot } from './ipc/bindings';

export function missingRequired(snapshot: PermissionsSnapshot | null): PermissionItem[] {
  if (!snapshot) return [];
  return snapshot.items.filter((item) => item.required && !item.granted);
}

export function footerPermissionWarning(snapshot: PermissionsSnapshot | null): string | null {
  const missing = missingRequired(snapshot);
  if (missing.length === 0) return null;
  return `${missing.map((item) => item.title).join('、')}未开启`;
}
