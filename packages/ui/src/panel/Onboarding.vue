<script setup lang="ts">
import { IconAccessible, IconCircleCheck, IconRefresh } from '@tabler/icons-vue';
import { useI18n } from 'vue-i18n';
import type { PermissionItem, PermissionsSnapshot } from '../ipc/bindings';

defineProps<{
  snapshot: PermissionsSnapshot;
}>();

const emit = defineEmits<{
  enable: [id: string];
  skip: [];
  start: [];
  restart: [];
  recheck: [];
}>();

const { t } = useI18n();

function itemIcon(item: PermissionItem) {
  return item.id === 'accessibility' ? IconAccessible : IconCircleCheck;
}

function copy(item: PermissionItem, field: 'title' | 'description' | 'hintOff' | 'hintOn') {
  return t(`permissions.${item.id}.${field}`);
}
</script>

<template>
  <div class="onboarding">
    <header class="onboarding-head">
      <p class="onboarding-kicker">{{ t('onboarding.kicker') }}</p>
      <h1>{{ t('onboarding.title') }}</h1>
      <p class="onboarding-lead">{{ t('onboarding.lead') }}</p>
    </header>

    <main class="onboarding-body">
      <article
        v-for="item in snapshot.items"
        :key="item.id"
        class="perm-row"
        :class="{ granted: item.granted }"
      >
        <span class="perm-icon" aria-hidden="true">
          <component :is="item.granted ? IconCircleCheck : itemIcon(item)" :size="18" :stroke-width="1.75" />
        </span>
        <div class="perm-copy">
          <div class="perm-title">
            <strong>{{ copy(item, 'title') }}</strong>
            <span class="perm-badge" :class="{ on: item.granted }">
              {{ item.granted ? t('onboarding.granted') : t('onboarding.missing') }}
            </span>
          </div>
          <p>{{ copy(item, 'description') }}</p>
          <p class="muted">{{ item.granted ? copy(item, 'hintOn') : copy(item, 'hintOff') }}</p>
        </div>
        <button
          v-if="!item.granted"
          class="action"
          type="button"
          @click="emit('enable', item.id)"
        >
          {{ t('onboarding.enable') }}
        </button>
      </article>

      <ol v-if="!snapshot.allRequiredGranted" class="onboarding-steps">
        <li>{{ t('onboarding.step1') }}</li>
        <li>{{ t('onboarding.step2') }}</li>
        <li>{{ t('onboarding.step3') }}</li>
      </ol>
    </main>

    <footer class="onboarding-foot">
      <button class="action subtle" type="button" @click="emit('skip')">{{ t('onboarding.later') }}</button>
      <span class="spacer"></span>
      <button class="action subtle" type="button" @click="emit('recheck')">
        <IconRefresh :size="14" :stroke-width="1.75" />
        {{ t('onboarding.recheck') }}
      </button>
      <button
        v-if="snapshot.allRequiredGranted"
        class="action subtle"
        type="button"
        @click="emit('restart')"
      >
        {{ t('onboarding.restart') }}
      </button>
      <button
        class="action"
        type="button"
        :disabled="!snapshot.allRequiredGranted"
        @click="emit('start')"
      >
        {{ t('onboarding.start') }}
      </button>
      <slot name="pin" />
    </footer>
  </div>
</template>
