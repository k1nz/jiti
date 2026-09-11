<script setup lang="ts">
import { PhAppleLogo, PhWindowsLogo } from '@phosphor-icons/vue'

defineProps<{
  showAll?: boolean
}>()

const { t } = useI18n()
const { macUrl, winUrl, platform, releasesUrl } = useDownloads()
const mounted = ref(false)

onMounted(() => {
  mounted.value = true
})

const macPrimary = computed(() => !mounted.value || platform.value !== 'win')
const winPrimary = computed(() => mounted.value && platform.value === 'win')
</script>

<template>
  <div class="cta-row">
    <a
      class="pill"
      :class="{ primary: macPrimary, recommended: mounted && platform === 'mac' }"
      :href="macUrl"
    >
      <PhAppleLogo :size="18" weight="regular" aria-hidden="true" />
      {{ t('cta.mac') }}
    </a>
    <a
      class="pill"
      :class="{ primary: winPrimary, recommended: mounted && platform === 'win' }"
      :href="winUrl"
    >
      <PhWindowsLogo :size="18" weight="regular" aria-hidden="true" />
      {{ t('cta.win') }}
    </a>
  </div>
  <a v-if="showAll" class="all-releases" :href="releasesUrl">{{ t('cta.all') }}</a>
</template>
