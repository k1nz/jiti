<script setup lang="ts">
import { IconBookmark, IconBookmarkFilled } from '@tabler/icons-vue';

withDefaults(
  defineProps<{
    saved?: boolean;
    label: string;
    disabled?: boolean;
    size?: number;
  }>(),
  { saved: false, disabled: false, size: 15 },
);

const emit = defineEmits<{
  click: [];
}>();
</script>

<template>
  <button
    class="icon-btn"
    :class="{ 'clip-on': saved }"
    type="button"
    :disabled="disabled"
    :aria-label="label"
    :title="label"
    :aria-pressed="saved"
    @click.stop="emit('click')"
  >
    <IconBookmarkFilled v-if="saved" :size="size" :stroke-width="0" />
    <IconBookmark v-else :size="size" :stroke-width="1.75" />
  </button>
</template>
