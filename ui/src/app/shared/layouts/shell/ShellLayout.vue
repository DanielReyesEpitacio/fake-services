<script setup lang="ts">
import { computed, ref } from 'vue'
import { useDisplay } from 'vuetify'
import { useI18n } from 'vue-i18n'
import AppLogo from '@/app/shared/components/AppLogo.vue'
import CopyButton from '@/app/shared/components/CopyButton.vue'
import { store } from '@/app/shared/state/live.ts'
import { useAppearance } from '@/app/configuration/appearance'
import type { ThemeMode } from '@/platform/adapters/vue/themes'

const { t } = useI18n()
const { mobile } = useDisplay()
const { preferences, setMode } = useAppearance()
const drawer = ref<boolean | null>(null)

const nav = [
  { to: '/', title: 'nav.dashboard', icon: 'mdi-view-dashboard-outline', exact: true },
  { to: '/servicios', title: 'nav.services', icon: 'mdi-server-outline' },
  { to: '/ejecuciones', title: 'nav.executions', icon: 'mdi-sitemap-outline' },
  { to: '/historial', title: 'nav.history', icon: 'mdi-history' },
  { to: '/espacios', title: 'nav.workspaces', icon: 'mdi-folder-outline' },
  { to: '/ajustes', title: 'nav.settings', icon: 'mdi-cog-outline' },
  { to: '/apariencia', title: 'nav.appearance', icon: 'mdi-palette' },
]

const modes: { value: ThemeMode; title: string; icon: string }[] = [
  { value: 'light', title: 'Claro', icon: 'mdi-weather-sunny' },
  { value: 'dark', title: 'Oscuro', icon: 'mdi-weather-night' },
  { value: 'system', title: 'Según el sistema', icon: 'mdi-monitor' },
]
const modeIcon = computed(() => modes.find((m) => m.value === preferences.mode)?.icon ?? 'mdi-monitor')
const listening = computed(() => store.connected && !!store.status?.listeners[0]?.listening)
</script>

<template>
  <VNavigationDrawer v-model="drawer" :permanent="!mobile" :temporary="mobile">
    <div class="d-flex align-center gap-3 px-2 pt-2 pb-4">
      <AppLogo style="width: 40px; height: 40px; flex-shrink: 0" />
      <div class="min-w-0">
        <div class="text-title-medium font-weight-bold">Fake Services</div>
        <div class="text-body-small text-medium-emphasis">{{ t('app.tagline') }}</div>
      </div>
    </div>

    <VList nav>
      <VListItem
        v-for="item in nav"
        :key="item.to"
        :to="item.to"
        :exact="item.exact"
        :prepend-icon="item.icon"
        :title="t(item.title)"
      />
    </VList>

    <template #append>
      <div v-if="store.status" class="pa-3 text-body-small">
        <div class="text-medium-emphasis">Espacio activo</div>
        <RouterLink to="/espacios" class="d-block text-truncate font-weight-medium text-high-emphasis" style="text-decoration: none">
          {{ store.status.workspaceName }}
        </RouterLink>
        <div class="text-disabled mt-2">v{{ store.status.version }}</div>
      </div>
    </template>
  </VNavigationDrawer>

  <VAppBar>
    <VAppBarNavIcon v-if="mobile" @click="drawer = !drawer" />
    <VAppBarTitle>
      <div v-if="store.status" class="d-flex align-center gap-2">
        <VIcon :color="listening ? 'success' : 'error'" size="12" icon="mdi-circle" />
        <span class="mono text-body-medium">{{ store.status.centralUrl }}</span>
        <CopyButton :text="store.status.centralUrl" />
        <VChip v-if="!store.connected" color="error" size="small" variant="tonal" prepend-icon="mdi-alert-circle-outline">
          Sin conexión con el núcleo
        </VChip>
      </div>
    </VAppBarTitle>
    <template #append>
      <VMenu>
        <template #activator="{ props }">
          <VBtn v-bind="props" :icon="modeIcon" variant="text" title="Modo de color" aria-label="Modo de color" />
        </template>
        <VList density="compact" min-width="200">
          <VListItem
            v-for="m in modes"
            :key="m.value"
            :prepend-icon="m.icon"
            :title="m.title"
            :active="preferences.mode === m.value"
            @click="setMode(m.value)"
          />
        </VList>
      </VMenu>
      <VBtn to="/apariencia" icon="mdi-palette" variant="text" title="Apariencia" aria-label="Apariencia" />
    </template>
  </VAppBar>

  <VMain>
    <RouterView :key="$route.path" />
  </VMain>
</template>
