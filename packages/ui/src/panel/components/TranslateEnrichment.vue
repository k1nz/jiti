<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { IconVolume } from '@tabler/icons-vue';
import type { EnglishEnrichment } from '../../ipc/bindings';

const props = defineProps<{
  word: string;
  enrichment?: EnglishEnrichment | null;
  loading?: boolean;
}>();

const { t } = useI18n();
const audioEl = ref<HTMLAudioElement | null>(null);
const speaking = ref(false);

const phonetic = computed(() => props.enrichment?.phonetic?.trim() || '');
const mnemonicEn = computed(() => props.enrichment?.mnemonic?.trim() || '');
const mnemonicZh = computed(() => props.enrichment?.mnemonicZh?.trim() || '');
const roots = computed(() => props.enrichment?.roots?.trim() || '');
const examples = computed(() =>
  (props.enrichment?.examples ?? []).filter((item) => item.text.trim()),
);
const senses = computed(() =>
  (props.enrichment?.senses ?? []).filter(
    (item) => item.definition.trim() || item.translation?.trim(),
  ),
);
const audioUrl = computed(() => props.enrichment?.audioDataUrl?.trim() || '');
const canPlay = computed(() => Boolean(props.word.trim()) && !props.loading);

function stopSpeech() {
  if (typeof window === 'undefined' || !('speechSynthesis' in window)) return;
  window.speechSynthesis.cancel();
  speaking.value = false;
}

function play() {
  if (audioUrl.value && audioEl.value) {
    stopSpeech();
    audioEl.value.currentTime = 0;
    void audioEl.value.play().catch(() => speak());
    return;
  }
  speak();
}

function speak() {
  if (typeof window === 'undefined' || !('speechSynthesis' in window)) return;
  const text = props.word.trim();
  if (!text) return;
  stopSpeech();
  const utterance = new SpeechSynthesisUtterance(text);
  utterance.lang = 'en-US';
  utterance.rate = 0.92;
  utterance.onend = () => {
    speaking.value = false;
  };
  utterance.onerror = () => {
    speaking.value = false;
  };
  speaking.value = true;
  window.speechSynthesis.speak(utterance);
}

onBeforeUnmount(() => {
  stopSpeech();
});
</script>

<template>
  <div
    class="word-card"
    :class="{ 'is-loading': loading }"
    data-tauri-drag-region="false"
    :aria-busy="loading ? 'true' : undefined"
  >
    <template v-if="loading">
      <div class="word-loading">
        <span class="spinner" aria-hidden="true"></span>
        <span>{{ t('translate.cardLoading') }}</span>
      </div>
      <div class="word-skel" aria-hidden="true"></div>
      <div class="word-skel word-skel-short" aria-hidden="true"></div>
      <div class="word-skel" aria-hidden="true"></div>
    </template>
    <template v-else>
      <div v-if="phonetic || canPlay" class="word-head">
        <span v-if="phonetic" class="word-phonetic">{{ phonetic }}</span>
        <button
          v-if="canPlay"
          class="icon-btn"
          type="button"
          :aria-label="t('translate.play')"
          :title="t('translate.play')"
          :aria-pressed="speaking"
          @click="play"
        >
          <IconVolume :size="14" :stroke-width="1.75" />
        </button>
        <audio v-if="audioUrl" ref="audioEl" :src="audioUrl" preload="none" />
      </div>

      <section v-if="mnemonicEn || mnemonicZh" class="word-section">
        <h3>{{ t('translate.mnemonic') }}</h3>
        <p v-if="mnemonicEn" class="word-en">{{ mnemonicEn }}</p>
        <p v-if="mnemonicZh" class="word-zh">{{ mnemonicZh }}</p>
      </section>

      <section v-if="roots" class="word-section">
        <h3>{{ t('translate.roots') }}</h3>
        <p>{{ roots }}</p>
      </section>

      <section v-if="senses.length" class="word-section">
        <h3>{{ t('translate.senses') }}</h3>
        <ul class="word-senses">
          <li v-for="(sense, index) in senses" :key="`${sense.definition}-${index}`">
            <div class="word-sense-en">
              <span v-if="sense.pos" class="word-pos">{{ sense.pos }}</span>
              <span>{{ sense.definition }}</span>
            </div>
            <span v-if="sense.translation" class="word-zh">{{ sense.translation }}</span>
          </li>
        </ul>
      </section>

      <section v-if="examples.length" class="word-section">
        <h3>{{ t('translate.examples') }}</h3>
        <ul class="word-examples">
          <li v-for="(example, index) in examples" :key="`${example.text}-${index}`">
            <span class="word-en">{{ example.text }}</span>
            <span v-if="example.translation" class="word-zh">{{ example.translation }}</span>
          </li>
        </ul>
      </section>
    </template>
  </div>
</template>
