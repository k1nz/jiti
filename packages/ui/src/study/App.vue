<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import {
  IconBookmark,
  IconNotebook,
  IconPlayerPlay,
  IconSparkles,
} from '@tabler/icons-vue';
import { commands } from '../ipc/bindings';
import type {
  Clip,
  EngineErrorPayload,
  Mistake,
  ReviewPlan,
  ReviewPlanItem,
} from '../ipc/bindings';
import { unwrap } from '../ipc/unwrap';
import { dictationCue, displayClip } from '../clips';
import { renderReviewMarkdown } from '../markdown';
import { formatTime } from '../format';

type View = 'library' | 'plan';

const { t, locale } = useI18n();
const view = ref<View>('plan');
const mistakes = ref<Mistake[]>([]);
const clips = ref<Clip[]>([]);
const plan = ref<ReviewPlan | null>(null);
const planError = ref<string | null>(null);
const generating = ref(false);
const pickedDay = ref<number | null>(null);
const sessionDay = ref<number | null>(null);
const sessionIndex = ref(0);
const answer = ref('');
const revealed = ref(false);
const matched = ref<boolean | null>(null);
const busy = ref(false);

const selectedDay = computed(() => {
  if (!plan.value) return null;
  const wanted = sessionDay.value ?? pickedDay.value ?? plan.value.currentDay;
  return plan.value.days.find((day) => day.dayIndex === wanted) ?? plan.value.days[0] ?? null;
});

const dayItems = computed(() => {
  if (!plan.value || !selectedDay.value) return [];
  return plan.value.items.filter((item) => item.dayIndex === selectedDay.value?.dayIndex);
});

const currentItem = computed<ReviewPlanItem | null>(() => {
  if (sessionDay.value == null) return null;
  return dayItems.value[sessionIndex.value] ?? null;
});

const dayProgress = computed(() => {
  const done = dayItems.value.filter((item) => item.result).length;
  return { done, total: dayItems.value.length };
});

const summaryHtml = computed(() =>
  plan.value ? renderReviewMarkdown(plan.value.summary) : '',
);

const inSession = computed(() => sessionDay.value != null);

const clipCards = computed(() =>
  clips.value.map((item) => ({
    item,
    ...displayClip(item.text, item.note),
  })),
);

const sessionPrompt = computed(() => {
  const item = currentItem.value;
  if (!item) return '';
  if (item.sourceKind !== 'clip') return item.promptText;
  const clip = clips.value.find((row) => row.id === item.sourceId);
  return dictationCue(clip?.text ?? item.expected, clip?.note ?? item.hint);
});

const sessionHint = computed(() => {
  const item = currentItem.value;
  if (!item || item.sourceKind === 'clip') return '';
  return item.hint?.trim() || '';
});

function kindLabel(kind: string | null | undefined) {
  if (kind === 'word') return t('study.kindWord');
  if (kind === 'phrase') return t('study.kindPhrase');
  if (kind === 'sentence') return t('study.kindSentence');
  const key = kind ? `errorType.${kind}` : 'errorType.other';
  const label = t(key);
  return label === key ? t('errorType.other') : label;
}

function statusLabel(status: string) {
  const key = `status.${status}`;
  const label = t(key);
  return label === key ? status : label;
}

async function reloadLibrary() {
  try {
    const [mistakeList, clipList] = await Promise.all([
      unwrap(commands.mistakesList({ status: null, errorType: null, timeRange: 'all', limit: null, offset: null })),
      unwrap(commands.clipsList({ status: null, kind: null, limit: null, offset: null })),
    ]);
    mistakes.value = mistakeList.items;
    clips.value = clipList.items;
  } catch (err) {
    planError.value = String(err);
  }
}

async function reloadPlan() {
  try {
    plan.value = await unwrap(commands.reviewPlanActive());
  } catch (err) {
    planError.value = String(err);
  }
}

async function generatePlan() {
  generating.value = true;
  planError.value = null;
  try {
    plan.value = await unwrap(commands.reviewPlanGenerate());
    view.value = 'plan';
    sessionDay.value = null;
    pickedDay.value = plan.value.days[0]?.dayIndex ?? null;
  } catch (err) {
    const payload = err as EngineErrorPayload;
    planError.value =
      payload && typeof payload === 'object' && 'message' in payload
        ? `${payload.code} · ${payload.message}`
        : String(err);
  } finally {
    generating.value = false;
  }
}

function startDay(dayIndex: number) {
  sessionDay.value = dayIndex;
  pickedDay.value = dayIndex;
  answer.value = '';
  revealed.value = false;
  matched.value = null;
  const items = plan.value?.items.filter((item) => item.dayIndex === dayIndex) ?? [];
  const firstOpen = items.findIndex((item) => !item.result);
  sessionIndex.value = firstOpen >= 0 ? firstOpen : 0;
  void reloadLibrary();
}

function exitSession() {
  sessionDay.value = null;
  answer.value = '';
  revealed.value = false;
  matched.value = null;
}

async function submitAnswer() {
  const item = currentItem.value;
  if (!item || busy.value) return;
  busy.value = true;
  try {
    const result = await unwrap(
      commands.reviewPlanGrade({
        itemId: item.id,
        answer: answer.value,
        outcome: '',
        markLearned: false,
      }),
    );
    plan.value = result.plan;
    matched.value = result.matched;
    revealed.value = true;
  } catch (err) {
    planError.value = String(err);
  } finally {
    busy.value = false;
  }
}

async function skipItem() {
  const item = currentItem.value;
  if (!item || busy.value) return;
  busy.value = true;
  try {
    const result = await unwrap(
      commands.reviewPlanGrade({
        itemId: item.id,
        answer: null,
        outcome: 'skipped',
        markLearned: false,
      }),
    );
    plan.value = result.plan;
    goNext();
  } catch (err) {
    planError.value = String(err);
  } finally {
    busy.value = false;
  }
}

async function markLearned() {
  const item = currentItem.value;
  if (!item || busy.value) return;
  busy.value = true;
  try {
    const result = await unwrap(
      commands.reviewPlanGrade({
        itemId: item.id,
        answer: null,
        outcome: 'mastered',
        markLearned: true,
      }),
    );
    plan.value = result.plan;
    await reloadLibrary();
    goNext();
  } catch (err) {
    planError.value = String(err);
  } finally {
    busy.value = false;
  }
}

function stayWrong() {
  goNext();
}

function goNext() {
  revealed.value = false;
  matched.value = null;
  answer.value = '';
  const next = sessionIndex.value + 1;
  if (next < dayItems.value.length) {
    sessionIndex.value = next;
    return;
  }
  sessionIndex.value = next;
}

async function completeDay() {
  if (!plan.value || selectedDay.value == null) return;
  busy.value = true;
  try {
    plan.value = await unwrap(
      commands.reviewPlanCompleteDay(plan.value.id, selectedDay.value.dayIndex),
    );
    exitSession();
    await reloadLibrary();
  } catch (err) {
    planError.value = String(err);
  } finally {
    busy.value = false;
  }
}

async function patchMistake(id: number, status: string) {
  try {
    await unwrap(commands.mistakesUpdate(id, { status }));
    await reloadLibrary();
  } catch (err) {
    planError.value = String(err);
  }
}

async function patchClip(id: number, status: string) {
  try {
    await unwrap(commands.clipsUpdate(id, { status }));
    await reloadLibrary();
  } catch (err) {
    planError.value = String(err);
  }
}

async function removeMistake(id: number) {
  await unwrap(commands.mistakesDelete(id));
  await reloadLibrary();
}

async function removeClip(id: number) {
  await unwrap(commands.clipsDelete(id));
  await reloadLibrary();
}

function onKeydown(event: KeyboardEvent) {
  if (event.key !== 'Escape') return;
  if (sessionDay.value != null) {
    event.preventDefault();
    exitSession();
  }
}

onMounted(() => {
  window.addEventListener('keydown', onKeydown);
  void reloadLibrary();
  void reloadPlan();
});

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown);
});
</script>

<template>
  <div class="study-shell">
    <nav class="study-nav" :aria-label="t('study.title')">
      <p class="study-brand">{{ t('study.title') }}</p>
      <button
        class="study-nav-btn"
        type="button"
        :class="{ on: view === 'plan' }"
        @click="view = 'plan'"
      >
        <IconSparkles :size="16" :stroke-width="1.75" />
        {{ t('study.plan') }}
      </button>
      <button
        class="study-nav-btn"
        type="button"
        :class="{ on: view === 'library' }"
        @click="view = 'library'"
      >
        <IconNotebook :size="16" :stroke-width="1.75" />
        {{ t('study.library') }}
      </button>
    </nav>

    <main class="study-main">
      <div v-if="planError" class="error-box">{{ planError }}</div>

      <template v-if="view === 'library'">
        <div class="study-split">
          <section class="study-col">
            <h2>{{ t('study.mistakes') }} · {{ mistakes.length }}</h2>
            <p v-if="mistakes.length === 0" class="muted">{{ t('mistakes.empty') }}</p>
            <ul v-else class="study-list">
              <li v-for="item in mistakes" :key="`m-${item.id}`" class="study-row">
                <p class="study-row-text">{{ item.sourceText }}</p>
                <div class="meta">
                  <span>{{ kindLabel(item.errorType) }}</span>
                  <span>{{ statusLabel(item.status) }}</span>
                  <span>{{ formatTime(item.createdAt, locale) }}</span>
                </div>
                <div class="mistake-actions">
                  <button
                    v-if="item.status === 'open'"
                    class="action subtle"
                    type="button"
                    @click="patchMistake(item.id, 'learned')"
                  >
                    {{ t('mistakes.learn') }}
                  </button>
                  <button class="action subtle" type="button" @click="removeMistake(item.id)">
                    {{ t('mistakes.remove') }}
                  </button>
                </div>
              </li>
            </ul>
          </section>
          <section class="study-col">
            <h2>{{ t('study.clips') }} · {{ clips.length }}</h2>
            <p v-if="clips.length === 0" class="muted">{{ t('study.emptyClips') }}</p>
            <ul v-else class="study-list">
              <li v-for="card in clipCards" :key="`c-${card.item.id}`" class="study-row">
                <p v-if="card.en" class="study-row-en">{{ card.en }}</p>
                <p
                  v-if="card.zh"
                  :class="card.en ? 'study-row-zh' : 'study-row-en'"
                >
                  {{ card.zh }}
                </p>
                <div class="meta">
                  <span>{{ kindLabel(card.item.kind) }}</span>
                  <span>{{ statusLabel(card.item.status) }}</span>
                </div>
                <div class="mistake-actions">
                  <button
                    v-if="card.item.status === 'open'"
                    class="action subtle"
                    type="button"
                    @click="patchClip(card.item.id, 'learned')"
                  >
                    {{ t('mistakes.learn') }}
                  </button>
                  <button class="action subtle" type="button" @click="removeClip(card.item.id)">
                    {{ t('mistakes.remove') }}
                  </button>
                </div>
              </li>
            </ul>
          </section>
        </div>
      </template>

      <template v-else>
        <div class="study-toolbar">
          <h1>{{ t('study.plan') }}</h1>
          <span class="spacer"></span>
          <button
            class="action"
            type="button"
            :disabled="generating"
            @click="generatePlan"
          >
            <IconSparkles :size="14" :stroke-width="1.75" />
            {{ plan ? t('study.regenerate') : t('study.generate') }}
          </button>
        </div>

        <div v-if="generating" class="state-box">
          <span class="spinner" aria-hidden="true"></span>
          <span>{{ t('study.generating') }}</span>
        </div>

        <template v-else-if="!plan">
          <div class="state-box">
            <IconBookmark :size="26" :stroke-width="1.5" />
            <span>{{ t('study.noPlan') }}</span>
          </div>
        </template>

        <template v-else-if="inSession">
          <div v-if="currentItem" class="study-session">
            <div class="meta">
              <span>{{ t('study.round', { n: selectedDay?.dayIndex }) }}</span>
              <span>{{ t('study.progress', dayProgress) }}</span>
              <span>{{
                currentItem.sourceKind === 'clip' ? t('study.dictation') : t('study.cloze')
              }}</span>
            </div>
            <p class="study-prompt">{{ sessionPrompt }}</p>
            <p v-if="sessionHint" class="study-hint">
              {{ t('study.hint') }} · {{ sessionHint }}
            </p>
            <input
              v-if="!revealed"
              v-model="answer"
              class="study-input"
              type="text"
              autocomplete="off"
              spellcheck="false"
              :placeholder="
                currentItem.sourceKind === 'clip'
                  ? t('study.dictationPlaceholder')
                  : t('study.clozePlaceholder')
              "
              @keydown.enter.prevent="submitAnswer"
            />
            <template v-else>
              <p class="study-verdict" :class="matched ? 'ok' : 'no'">
                {{ matched ? t('study.correct') : t('study.wrong') }}
              </p>
              <p class="study-prompt">{{ currentItem.expected }}</p>
            </template>
            <div class="study-actions">
              <button
                v-if="!revealed"
                class="action"
                type="button"
                :disabled="busy || !answer.trim()"
                @click="submitAnswer"
              >
                <IconPlayerPlay :size="14" :stroke-width="1.75" />
                {{ t('study.check') }}
              </button>
              <button
                v-if="!revealed"
                class="action subtle"
                type="button"
                :disabled="busy"
                @click="skipItem"
              >
                {{ t('study.skip') }}
              </button>
              <button
                v-if="revealed"
                class="action"
                type="button"
                :disabled="busy"
                @click="markLearned"
              >
                {{ t('study.mastered') }}
              </button>
              <button
                v-if="revealed"
                class="action subtle"
                type="button"
                @click="stayWrong"
              >
                {{ t('study.again') }}
              </button>
              <button class="action subtle" type="button" @click="exitSession">
                {{ t('study.exitSession') }}
              </button>
            </div>
          </div>
          <div v-else class="study-session">
            <p class="study-prompt">{{ t('study.dayDone') }}</p>
            <div class="study-actions">
              <button class="action" type="button" :disabled="busy" @click="completeDay">
                {{ t('study.completeDay') }}
              </button>
              <button class="action subtle" type="button" @click="exitSession">
                {{ t('study.exitSession') }}
              </button>
            </div>
          </div>
        </template>

        <template v-else>
          <div class="study-days">
            <button
              v-for="day in plan.days"
              :key="day.dayIndex"
              class="study-day"
              type="button"
              :class="{ on: selectedDay?.dayIndex === day.dayIndex, done: day.status === 'done' }"
              @click="pickedDay = day.dayIndex"
            >
              {{ t('study.round', { n: day.dayIndex }) }} · {{ day.title }}
            </button>
          </div>
          <p v-if="selectedDay?.drill" class="study-hint">{{ selectedDay.drill }}</p>
          <div
            v-if="plan.summary"
            class="review-summary markdown-body study-summary"
            v-html="summaryHtml"
          ></div>
          <div class="study-actions">
            <button
              class="action"
              type="button"
              :disabled="!selectedDay || dayItems.length === 0"
              @click="selectedDay && startDay(selectedDay.dayIndex)"
            >
              <IconPlayerPlay :size="14" :stroke-width="1.75" />
              {{ t('study.startDay') }}
            </button>
          </div>
        </template>
      </template>
    </main>
  </div>
</template>
