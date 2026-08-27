<script setup lang="ts">
import { IconAccessible, IconCircleCheck, IconRefresh } from '@tabler/icons-vue';
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

function itemIcon(item: PermissionItem) {
  return item.id === 'accessibility' ? IconAccessible : IconCircleCheck;
}
</script>

<template>
  <div class="onboarding">
    <header class="onboarding-head">
      <p class="onboarding-kicker">首次使用</p>
      <h1>开启必要权限</h1>
      <p class="onboarding-lead">
        Jiti 需要读取你在其他应用中选中的文字，才能一键翻译。授权只需一次，之后可随时在设置里查看。
      </p>
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
            <strong>{{ item.title }}</strong>
            <span class="perm-badge" :class="{ on: item.granted }">
              {{ item.granted ? '已开启' : '未开启' }}
            </span>
          </div>
          <p>{{ item.description }}</p>
          <p v-if="item.hint" class="muted">{{ item.hint }}</p>
        </div>
        <button
          v-if="!item.granted"
          class="action"
          type="button"
          @click="emit('enable', item.id)"
        >
          去开启
        </button>
      </article>

      <ol v-if="!snapshot.allRequiredGranted" class="onboarding-steps">
        <li>点击「去开启」，在系统设置的列表里找到 Jiti</li>
        <li>打开右侧开关</li>
        <li>回到这里，状态会自动更新</li>
      </ol>
    </main>

    <footer class="onboarding-foot">
      <button class="action subtle" type="button" @click="emit('skip')">稍后再说</button>
      <span class="spacer"></span>
      <button class="action subtle" type="button" @click="emit('recheck')">
        <IconRefresh :size="14" :stroke-width="1.75" />
        重新检测
      </button>
      <button
        v-if="snapshot.allRequiredGranted"
        class="action subtle"
        type="button"
        @click="emit('restart')"
      >
        重启应用
      </button>
      <button
        class="action"
        type="button"
        :disabled="!snapshot.allRequiredGranted"
        @click="emit('start')"
      >
        开始使用
      </button>
      <slot name="pin" />
    </footer>
  </div>
</template>
