import { commands, events, type PreferencesSnapshot } from './ipc/bindings';
import { applyTheme } from './appearance';
import { syncI18n } from './i18n';

const FALLBACK: PreferencesSnapshot = {
  locale: 'system',
  theme: 'system',
  autostart: { enabled: false, hint: null },
  focusOnInvoke: true,
};

let current: PreferencesSnapshot = FALLBACK;

export function currentPreferences(): PreferencesSnapshot {
  return current;
}

export function applyPreferences(snapshot: PreferencesSnapshot) {
  current = snapshot;
  applyTheme(snapshot.theme);
  syncI18n(snapshot.locale);
}

export async function bootPreferences() {
  try {
    applyPreferences(await commands.preferencesSnapshot());
  } catch {
    applyPreferences(FALLBACK);
  }
  return events.preferencesChanged.listen((event) => {
    applyPreferences(event.payload.snapshot);
  });
}
