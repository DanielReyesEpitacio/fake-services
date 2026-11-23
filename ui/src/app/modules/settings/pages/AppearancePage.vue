<script setup lang="ts">
import { computed, ref } from 'vue'
import PageHeader from '@/app/shared/components/PageHeader.vue'
import { ACCENTS, LOOK_AND_FEELS, findLookAndFeel, useAppearance } from '@/app/configuration/appearance'
import type { ThemeMode } from '@/platform/adapters/vue/themes'

const { preferences, setMode, apply } = useAppearance()

const lookAndFeel = ref<string>(preferences.lookAndFeel)
const accent = ref<string>(preferences.accent)
const pending = computed(() => lookAndFeel.value !== preferences.lookAndFeel || accent.value !== preferences.accent)

const mode = computed({
  get: () => preferences.mode,
  set: (m: ThemeMode) => setMode(m),
})

function accentColor(id: string): string {
  const a = ACCENTS.find((x) => x.id === id)
  return a?.light?.primary ?? findLookAndFeel(lookAndFeel.value).swatch[2]
}

function applyChanges() {
  apply({ lookAndFeel: lookAndFeel.value as typeof preferences.lookAndFeel, accent: accent.value })
}
</script>

<template>
  <div style="max-width: 1100px">
    <PageHeader title="Apariencia" subtitle="Elige el look and feel, el modo de color y el acento de la interfaz. Se guarda en este equipo.">
      <template #actions>
        <VBtn v-if="pending" variant="text" @click="(lookAndFeel = preferences.lookAndFeel), (accent = preferences.accent)">Descartar</VBtn>
        <VBtn color="primary" :disabled="!pending" prepend-icon="mdi-check" @click="applyChanges">Aplicar y recargar</VBtn>
      </template>
    </PageHeader>

    <VCard title="Modo de color" subtitle="Se aplica al instante." class="mb-4">
      <VCardText>
        <VBtnToggle v-model="mode" mandatory color="primary" variant="outlined" divided>
          <VBtn value="light" prepend-icon="mdi-weather-sunny">Claro</VBtn>
          <VBtn value="dark" prepend-icon="mdi-weather-night">Oscuro</VBtn>
          <VBtn value="system" prepend-icon="mdi-monitor">Según el sistema</VBtn>
        </VBtnToggle>
      </VCardText>
    </VCard>

    <VCard title="Look and feel" subtitle="Cambia formas, densidad, tipografía y materiales. Requiere recargar la interfaz." class="mb-4">
      <VCardText>
        <VRow>
          <VCol v-for="lf in LOOK_AND_FEELS" :key="lf.id" cols="12" sm="6" lg="4">
            <button
              type="button"
              class="lf-option"
              :class="{ selected: lookAndFeel === lf.id }"
              :aria-pressed="lookAndFeel === lf.id"
              @click="lookAndFeel = lf.id"
            >
              <div class="lf-preview" :style="{ background: lf.swatch[0] }">
                <div class="lf-preview-nav" :style="{ background: lf.swatch[1], borderRadius: `${Math.min(lf.radius, 12)}px` }" />
                <div class="lf-preview-panel" :style="{ background: lf.swatch[1], borderRadius: `${Math.min(lf.radius, 12)}px` }">
                  <span class="lf-preview-line" />
                  <span class="lf-preview-line short" />
                  <span
                    class="lf-preview-button"
                    :style="{ background: accent === 'native' ? lf.swatch[2] : accentColor(accent), borderRadius: `${Math.min(lf.radius, 10)}px` }"
                  />
                </div>
              </div>
              <div class="d-flex align-center gap-2 mt-3">
                <span class="text-title-small font-weight-bold">{{ lf.name }}</span>
                <VChip v-if="lf.id === 'cupertinoGlass'" size="x-small" variant="tonal">Predeterminado</VChip>
                <VSpacer />
                <VIcon v-if="lookAndFeel === lf.id" icon="mdi-check-circle" color="primary" size="20" />
              </div>
              <div class="text-body-small text-medium-emphasis mt-1">{{ lf.description }}</div>
            </button>
          </VCol>
        </VRow>
      </VCardText>
    </VCard>

    <VCard title="Color de acento" subtitle="Botones principales, selección y foco.">
      <VCardText>
        <VChipGroup v-model="accent" mandatory column>
          <VChip v-for="a in ACCENTS" :key="a.id" :value="a.id" filter variant="outlined">
            <template #prepend>
              <span class="accent-dot me-2" :style="{ background: accentColor(a.id) }" />
            </template>
            {{ a.name }}
          </VChip>
        </VChipGroup>
      </VCardText>
    </VCard>
  </div>
</template>

<style scoped>
.lf-option {
  display: block;
  width: 100%;
  text-align: left;
  padding: 12px;
  border-radius: 14px;
  border: 2px solid rgba(var(--v-border-color), var(--v-border-opacity));
  background: transparent;
  color: inherit;
  cursor: pointer;
  transition: border-color 0.15s;
}
.lf-option:hover {
  border-color: rgba(var(--v-theme-primary), 0.5);
}
.lf-option.selected {
  border-color: rgb(var(--v-theme-primary));
}
.lf-option:focus-visible {
  outline: 2px solid rgb(var(--v-theme-primary));
  outline-offset: 2px;
}
.lf-preview {
  display: flex;
  gap: 6px;
  height: 92px;
  padding: 8px;
  border-radius: 10px;
}
.lf-preview-nav {
  width: 28%;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.08);
}
.lf-preview-panel {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 10px;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.08);
}
.lf-preview-line {
  height: 6px;
  width: 80%;
  border-radius: 3px;
  background: rgba(0, 0, 0, 0.12);
}
.lf-preview-line.short {
  width: 50%;
}
.lf-preview-button {
  margin-top: auto;
  width: 42%;
  height: 14px;
}
.accent-dot {
  display: inline-block;
  width: 14px;
  height: 14px;
  border-radius: 50%;
}
</style>
