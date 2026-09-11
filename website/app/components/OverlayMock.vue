<script setup lang="ts">
import { PhClockCounterClockwise, PhMagnifyingGlass, PhNotebook, PhTextAa, PhTranslate } from '@phosphor-icons/vue'

const { t } = useI18n()
</script>

<template>
  <section class="stage" :aria-label="t('mock.label')">
    <p class="stage-label">{{ t('mock.label') }}</p>
    <div class="overlay">
      <div class="ov-search">
        <PhMagnifyingGlass :size="18" weight="regular" aria-hidden="true" />
        <span class="ov-query">{{ t('mock.query') }}</span>
      </div>
      <div class="ov-tabs">
        <span class="ov-tab on">
          <PhTranslate :size="16" weight="regular" aria-hidden="true" />
          <span>{{ t('tabs.translate') }}</span>
        </span>
        <span class="ov-tab">
          <PhTextAa :size="16" weight="regular" aria-hidden="true" />
          <span>{{ t('tabs.grammar') }}</span>
        </span>
        <span class="ov-tab">
          <PhNotebook :size="16" weight="regular" aria-hidden="true" />
          <span>{{ t('tabs.mistakes') }}</span>
        </span>
        <span class="ov-tab">
          <PhClockCounterClockwise :size="16" weight="regular" aria-hidden="true" />
          <span>{{ t('tabs.history') }}</span>
        </span>
      </div>
      <div class="ov-body">
        <div class="ov-toolbar">
          <span>{{ t('mock.pair') }}</span>
          <span class="ov-run">{{ t('tabs.translate') }}</span>
        </div>
        <div class="ov-out">
          <p>{{ t('mock.output') }}</p>
          <div class="ov-meta">
            <span>DeepL</span>
            <span>en → zh</span>
            <span>312 ms</span>
          </div>
        </div>
      </div>
      <div class="ov-status">
        <span class="dot" aria-hidden="true" />
        <span>{{ t('mock.status') }}</span>
        <span class="spacer" />
        <kbd>Esc</kbd>
        <span>{{ t('mock.hide') }}</span>
      </div>
    </div>
  </section>
</template>
