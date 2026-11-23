// Diálogo de confirmación global basado en promesas.

import { reactive } from 'vue'

export const confirmState = reactive({
  open: false,
  title: '',
  text: '',
  confirmText: 'Confirmar',
  danger: false,
})

let resolver: ((v: boolean) => void) | null = null

export function confirmAction(opts: {
  title: string
  text: string
  confirmText?: string
  danger?: boolean
}): Promise<boolean> {
  resolver?.(false)
  Object.assign(confirmState, {
    open: true,
    title: opts.title,
    text: opts.text,
    confirmText: opts.confirmText ?? 'Confirmar',
    danger: opts.danger ?? false,
  })
  return new Promise((resolve) => (resolver = resolve))
}

export function resolveConfirm(value: boolean) {
  confirmState.open = false
  resolver?.(value)
  resolver = null
}
