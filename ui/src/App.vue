<script setup lang="ts">
import { onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Activity, FolderGit2, History, LayoutDashboard, Server, Settings } from 'lucide-vue-next'
import AppLogo from './components/AppLogo.vue'
import ToastHost from './components/ToastHost.vue'
import CopyButton from './components/CopyButton.vue'
import { startLive, store } from './lib/store'

const { t } = useI18n()

const nav = [
  { to: '/', label: 'nav.dashboard', icon: LayoutDashboard },
  { to: '/servicios', label: 'nav.services', icon: Server },
  { to: '/historial', label: 'nav.history', icon: History },
  { to: '/espacios', label: 'nav.workspaces', icon: FolderGit2 },
  { to: '/ajustes', label: 'nav.settings', icon: Settings },
]

onMounted(startLive)
</script>

<template>
  <div class="flex h-full">
    <aside class="flex w-60 shrink-0 flex-col bg-ink text-zinc-300">
      <div class="flex items-center gap-2.5 px-4 py-4">
        <AppLogo class="size-9" />
        <div class="leading-tight">
          <div class="font-semibold tracking-tight text-white">Fake Services</div>
          <div class="text-[11px] text-zinc-500">{{ t('app.tagline') }}</div>
        </div>
      </div>

      <nav class="mt-2 flex flex-col gap-0.5 px-2">
        <RouterLink
          v-for="item in nav"
          :key="item.to"
          :to="item.to"
          class="flex items-center gap-2.5 rounded-lg px-3 py-2 text-sm transition hover:bg-ink-3 hover:text-white"
          :class="{
            'bg-ink-3 text-white': item.to === '/' ? $route.path === '/' : $route.path.startsWith(item.to),
          }"
        >
          <component :is="item.icon" class="size-4" />
          {{ t(item.label) }}
        </RouterLink>
      </nav>

      <div class="mt-auto space-y-2 border-t border-ink-3 px-4 py-4 text-xs">
        <div v-if="store.status" class="space-y-1">
          <div class="text-zinc-500">Espacio activo</div>
          <RouterLink to="/espacios" class="block truncate font-medium text-zinc-200 hover:text-white">
            {{ store.status.workspaceName }}
          </RouterLink>
        </div>
        <div v-if="store.status" class="flex items-center gap-1.5">
          <span
            class="size-2 rounded-full"
            :class="store.connected && store.status.listeners[0]?.listening ? 'bg-live' : 'bg-fail'"
          />
          <code class="truncate text-zinc-300">{{ store.status.centralUrl }}</code>
          <CopyButton :text="store.status.centralUrl" class="text-zinc-500 hover:text-white" />
        </div>
        <div v-if="!store.connected" class="flex items-center gap-1.5 text-fail">
          <Activity class="size-3.5" /> Sin conexión con el núcleo
        </div>
        <div v-if="store.status" class="text-zinc-600">v{{ store.status.version }}</div>
      </div>
    </aside>

    <main class="min-w-0 flex-1 overflow-y-auto">
      <RouterView :key="$route.path" />
    </main>
    <ToastHost />
  </div>
</template>
