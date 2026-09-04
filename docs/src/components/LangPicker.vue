<script setup lang="ts">
import { ref, computed } from "vue";
import { useFloating, offset, flip, shift, autoUpdate } from "@floating-ui/vue";
import { useCodeLanguage } from "~/composables/useCodeLanguage";

const props = defineProps<{
  epId: string;
  openId: string | null;
}>();

const emit = defineEmits<{
  toggle: [epId: string];
  select: [langId: string];
}>();

const { selected, current, languages } = useCodeLanguage();

const reference = ref<HTMLElement | null>(null);
const floating = ref<HTMLElement | null>(null);

const isOpen = computed(() => props.openId === props.epId);

const { floatingStyles } = useFloating(reference, floating, {
  placement: "bottom-end",
  middleware: [offset(4), flip(), shift({ padding: 8 })],
  whileElementsMounted: autoUpdate,
  open: isOpen,
});
</script>

<template>
  <div class="lang-picker">
    <button ref="reference" class="lang-picker-btn" @click.stop="emit('toggle', epId)">
      {{ current.label }}
      <svg
        class="lang-picker-chevron"
        :class="{ 'lang-picker-chevron--open': isOpen }"
        width="10" height="6" viewBox="0 0 10 6" fill="none"
      >
        <path d="M1 1L5 5L9 1" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
      </svg>
    </button>
    <Teleport to="body">
      <div v-if="isOpen" ref="floating" :style="floatingStyles" class="lang-dropdown">
        <button
          v-for="lang in languages"
          :key="lang.id"
          class="lang-dropdown-item"
          :class="{ 'lang-dropdown-item--active': selected === lang.id }"
          @click="emit('select', lang.id)"
        >
          {{ lang.label }}
        </button>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.lang-picker { position: relative; }

.lang-picker-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  font-family: var(--font-ui);
  font-size: 0.6875rem;
  font-weight: 500;
  color: #7d8b9d;
  background: none;
  border: 1px solid rgba(255, 255, 255, 0.08);
  padding: 3px var(--space-3);
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
  white-space: nowrap;
}

.lang-picker-btn:hover {
  color: #b4c1d1;
  border-color: rgba(255, 255, 255, 0.15);
  background: rgba(255, 255, 255, 0.04);
}

.lang-picker-chevron {
  transition: transform var(--duration-fast) var(--ease-out);
}

.lang-picker-chevron--open { transform: rotate(180deg); }

.lang-dropdown {
  z-index: 1000;
  min-width: 130px;
  background: #1e2a3a;
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: var(--radius-md);
  padding: 4px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  display: flex;
  flex-direction: column;
}

.lang-dropdown-item {
  font-family: var(--font-ui);
  font-size: var(--text-sm);
  font-weight: 500;
  color: #8b9cb5;
  background: none;
  border: none;
  padding: 6px 12px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  text-align: left;
  transition: all var(--duration-fast) var(--ease-out);
}

.lang-dropdown-item:hover { background: rgba(255, 255, 255, 0.06); color: #fff; }
.lang-dropdown-item--active { color: #fff; background: rgba(255, 255, 255, 0.1); }
</style>
