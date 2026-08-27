import type { PermissionItem, PermissionsSnapshot } from './ipc/bindings';

export function missingRequired(snapshot: PermissionsSnapshot | null): PermissionItem[] {
  if (!snapshot) return [];
  return snapshot.items.filter((item) => item.required && !item.granted);
}

export function footerPermissionWarning(
  snapshot: PermissionsSnapshot | null,
  t?: (key: string, values?: Record<string, unknown>) => string,
): string | null {
  const missing = missingRequired(snapshot);
  if (missing.length === 0) return null;
  const join = t ? t('footer.listJoin') : '、';
  const names = missing
    .map((item) => (t ? t(`permissions.${item.id}.title`) : item.title))
    .join(join);
  return t ? t('footer.permissionWarning', { names }) : `${names}未开启`;
}
