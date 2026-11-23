# Temas empresariales

## Imports aislados

La entrada `@/platform/adapters/vue/themes` exporta únicamente la infraestructura
compartida y los tipos. Ya no reexporta factories ni la demostración: importa
cada apariencia desde su archivo para evitar cargar CSS de otros perfiles.

```ts
import { createPlatformTheme } from '@/platform/adapters/vue/themes'
import { createCupertinoGlassLookAndFeel } from '@/platform/adapters/vue/themes/cupertinoGlass'

app.use(createPlatformTheme({ blueprint: createCupertinoGlassLookAndFeel() }))
```

Entradas disponibles: `atlas`, `meridian`, `harbor`, `atlassian`, `airbnb`,
`jetbrains`, `fluent`, `jetbrainsGlass` y `cupertinoGlass`.
Solo `cupertinoGlass` importa Manrope y Phosphor. `jetbrainsGlass` usa su propia base `jetbrainsGlassBase.css`, sin importar
el preset ni el CSS del perfil JetBrains actual. La base común carga los estilos
nativos de Vuetify y MDI, necesario para los iconos existentes y el fallback.

Importar varias entradas carga los recursos de todas ellas; no las agrupes en
un barrel compartido si quieres mantener este aislamiento. Esto aísla la carga
por entrada, no crea un Shadow DOM ni descarga CSS al cambiar de tema en runtime.

Factories de apariencia consumidas por `createPlatformTheme`, que encapsula Vuetify, sus estilos nativos, componentes y directivas. Cada llamada crea una configuración independiente.
Airbnb y JetBrains complementan los defaults nativos con `airbnb.css` y `jetbrains.css`, importados desde sus entradas independientes. No requieren dependencias nuevas ni `!important`.
Fluent añade `fluent.css`, también automático y acotado a sus propias clases.

| Factory                        | Características                                                                                              |
| ------------------------------ | ------------------------------------------------------------------------------------------------------------ |
| `createAtlasLookAndFeel()`     | Consola angular: radios 0px, navegación azul oscura, densidad compacta y títulos monoespaciados.             |
| `createMeridianLookAndFeel()`  | Editorial: radios 8px, papel cálido, títulos serif y contornos finos.                                        |
| `createHarborLookAndFeel()`    | Orgánico: radios 24px, superficies verdes suaves, controles amplios y sin sombras.                           |
| `createAirbnbLookAndFeel()`    | Tarjetas de 24px con sombras suaves, campos de 56px, botones de 48px y chips tipo píldora.                   |
| `createJetBrainsLookAndFeel()` | Paneles de 2px, cabeceras de herramientas, campos y filas de 32px, botones de 30px y títulos monoespaciados. |
| `createFluentLookAndFeel()`    | Fondo luminoso, tarjetas de vidrio, navegación flotante, selección lateral y menús acrílicos.                |

```ts
import { createApp } from 'vue'
import App from '@/App.vue'
import { createPlatformTheme } from '@/platform/adapters/vue/themes'
import { createAtlasLookAndFeel } from '@/platform/adapters/vue/themes/atlas'

const app = createApp(App)

app.use(
  createPlatformTheme(
    createAtlasLookAndFeel({
      colors: { primary: '#2457A7', secondary: '#54657B' },
      dark: { colors: { primary: '#A6C8FF', secondary: '#B5C5DA' } },
    }),
  ),
)

app.mount('#app')
// La raíz visual debe usar <VApp>.
```

Sin argumentos, cada factory incluye sus colores predeterminados en ambos modos.
`colors` personaliza ambos modos; `light.colors` y `dark.colors` tienen prioridad.
Cambiar primary no genera un tono oscuro alternativo: configúralo explícitamente si
tu marca necesita otro tono para ese modo. Vuetify calcula los colores `on-*` que
no se proporcionen; puedes sobrescribirlos con los mismos objetos `colors`.
Verifica el contraste de los colores que use cada empresa.

Las variables se ajustan con `light.variables` y `dark.variables`. Los defaults
se combinan por componente y por prop; una prop que sea un objeto se reemplaza
completa. Usa props explícitas en las vistas para excepciones locales.

Los nombres registrados son `light` y `dark`. Desde un componente:

```ts
import { useTheme } from 'vuetify'
const theme = useTheme()
await theme.toggle(['light', 'dark'])
// También: await theme.change('dark')
```

Elige una factory al configurar la app. Cambiar el modo cambia los colores, pero
conserva los defaults del estilo elegido. Cambiar entre Atlas, Meridian y Harbor
también requiere aplicar sus defaults: no basta con cambiar el nombre del tema.
Los iconos y las fuentes se configuran por separado, salvo Cupertino Glass, que
incluye Manrope y su integración Phosphor al elegir esa factory.

Referencias: https://vuetifyjs.com/en/features/theme/ y
https://vuetifyjs.com/en/features/global-configuration/

Sin personalización: app.use(createPlatformTheme()). Usa Atlas por defecto.

## Vista de demostración

`ThemeConcept` contiene su propio `VApp` y layout. Úsalo directamente en App.vue,
sin otro `VApp` o `VMain` alrededor:

```vue
<script setup lang="ts">
import { ThemeConcept } from '@/platform/adapters/vue/themes/themeConcept'
</script>
<template>
  <ThemeConcept company-name="Mi empresa" />
</template>
```

Las vistas usan directamente los componentes nativos de Vuetify, como `VBtn`,
`VCard` y `VNavigationDrawer`. Personaliza sus props con
`defaults: { VBtn: { rounded: 'pill' } }`.

En main.ts registra una sola instancia de `createPlatformTheme`. Para comparar
estilos cambia `createAtlasLookAndFeel()` por `createMeridianLookAndFeel()` o `createHarborLookAndFeel()`.
La vista permite alternar claro/oscuro, buscar proyectos y probar controles con datos
locales. Los destinos de navegación sin implementación aparecen deshabilitados.

## Reglas de cada sistema

`systemDefaults.ts` proporciona la regla base de radio para tarjetas, campos, botones,
chips, avatares, alertas, elementos de navegación y barras de progreso. Las
superficies estructurales de la ventana permanecen rectas. Airbnb y JetBrains sobrescriben estos defaults por componente para tener jerarquías de forma distintas.
Los CSS de cada perfil usan clases de perfil, variables CSS nativas y colores del modo activo para las sombras, cabeceras de tabla, tipografía y dimensiones compactas. Los menús y diálogos reciben su clase mediante `contentClass`, incluso cuando Vuetify los teletransporta fuera del árbol de la app.
La vista no fuerza radios ni densidades; las excepciones explícitas siguen siendo
posibles mediante props o `defaults` de cada empresa.

Los controles base no tienen sombras ni ripple; Airbnb añade sombras suaves a sus tarjetas y superficies flotantes. Los campos conservan las variantes y etiquetas nativas de Vuetify. `font-body` y `font-heading` emplean fuentes del
sistema sin descargar archivos. Los tokens `navigation`, `header` e `input` pueden
personalizarse por empresa igual que `primary` y `secondary`.

## Tema inspirado en Atlassian

```ts
import { createPlatformTheme } from '@/platform/adapters/vue/themes'
import { createAtlassianLookAndFeel } from '@/platform/adapters/vue/themes/atlassian'

app.use(
  createPlatformTheme({
    blueprint: createAtlassianLookAndFeel(),
    theme: {
      defaultTheme: 'system', // 'light' | 'dark' | 'system'
      themes: {
        light: { dark: false, colors: { primary: '#0C66E4' } },
        dark: { dark: true, colors: { primary: '#85B8FF' } },
      },
    },
  }),
)
```

`createAtlassianLookAndFeel()` es una adaptación propia usando Vuetify, no un producto
oficial ni una reproducción exacta del sistema de Atlassian. Prioriza neutros,
jerarquía funcional y acciones azules. En claro: fondo #FAFAFA y superficie #FFFFFF.
En oscuro: fondo #1F1F21 y superficie #29292C. No tiñe el fondo de azul.

Usa controles compactos, encabezados discretos, botones de 32px, bordes finos,
radio coherente de 6px y superficies planas. Conserva las mismas opciones de personalización de las demás factories.
El radio uniforme es una decisión de este proyecto; Atlassian tiene una escala
más amplia de radios según el papel del componente.

Referencias usadas para la adaptación:
https://atlassian.design/foundations/color/
https://atlassian.design/foundations/elevation/
https://atlassian.design/foundations/radius/

## Configuración nativa en la app

Las factories crean la apariencia (paletas clara y oscura y defaults).
El modo se elige en `createPlatformTheme({ blueprint: createAtlassianLookAndFeel(),
theme: { defaultTheme: 'system' } })`, mediante las opciones nativas de Vuetify.
`light` fija el modo claro, `dark` el oscuro y `system` sigue la preferencia del sistema.
`theme.themes.light` y `theme.themes.dark` permiten personalizar cada paleta desde la app;
Vuetify combina estas opciones con el blueprint. `createPlatformTheme` devuelve la
instancia nativa de Vuetify, incluida su API `theme`. No se usa `mode` en las factories.

## Inputs nativos

Los formularios usan los controles de Vuetify y sus props, eventos y slots:

```vue
<VTextField v-model="email" label="Correo electrónico" type="email" />
<VSelect v-model="selection" label="Opciones" :items="items" />
<VTextarea v-model="notes" label="Notas" />
<VCheckbox v-model="accepted" label="Acepto las condiciones" />
```

Las etiquetas se renderizan con el comportamiento nativo de cada control.
Los defaults del tema y de `VDefaultsProvider` siguen aplicándose.

## Fluent expresivo

```ts
import { createPlatformTheme } from '@/platform/adapters/vue/themes'
import { createFluentLookAndFeel } from '@/platform/adapters/vue/themes/fluent'

const vuetify = createPlatformTheme({
  blueprint: createFluentLookAndFeel(),
  theme: { defaultTheme: 'system' },
})
```

Adaptación visual inspirada en Fluent, con una composición más expresiva. El fondo
combina luces azul, lavanda y menta; los paneles semitransparentes de 20px usan
bordes iluminados, sombras y desenfoque. La navegación flota visualmente dentro
del espacio que reserva Vuetify: no se cambian sus offsets ni su comportamiento.
El modo oscuro tiene superficies azul noche y reflejos fríos.

Los tokens `glow-cool`, `glow-warm` y `glow-mint` personalizan las luces.
Las variables `fluent-glass-opacity`, `fluent-stroke-opacity`,
`fluent-glow-opacity` y `fluent-shadow-opacity` ajustan la intensidad por modo.
Los selects, autocompletes y comboboxes pasan el perfil a sus menús mediante
`menuProps.contentClass`; los menús y diálogos independientes usan `contentClass`.

No implementa Mica del sistema operativo: el material se aproxima con CSS.
Si el navegador no soporta `backdrop-filter`, muestra superficies sólidas.
Respeta movimiento y transparencia reducidos, y los colores forzados del sistema.
En móvil las tarjetas no usan desenfoque para reducir el coste de renderizado.
No descarga fuentes, instala plugins ni requiere recompilar Sass.

## JetBrains Glass

```ts
import { createPlatformTheme } from '@/platform/adapters/vue/themes'
import { createJetBrainsGlassLookAndFeel } from '@/platform/adapters/vue/themes/jetbrainsGlass'

const vuetify = createPlatformTheme({
  blueprint: createJetBrainsGlassLookAndFeel(),
  theme: { defaultTheme: 'system' },
})
```

Variante independiente de JetBrains: mantiene controles y filas de 32px,
barra de 48px y cabeceras monoespaciadas. Agrega paneles de 8px con vidrio
esmerilado, bordes iluminados y un fondo con luces violetas, cian y magenta
sobre una cuadrícula sutil. No cambia la factory JetBrains original.

`jetbrainsGlass.css` se importa automáticamente y sus clases no se aplican a
los demás estilos. Los menús de select, autocomplete y combobox conservan el
material al teletransportarse. Los colores `glass-violet`, `glass-cyan` y
`glass-pink`, y las variables `jb-glass-opacity`, `jb-glass-stroke`,
`jb-glass-glow` y `jb-glass-shadow` permiten ajustar el efecto por modo.

Con transparencia reducida o sin soporte de `backdrop-filter` usa superficies
sólidas. En móvil se desactiva el desenfoque de las tarjetas para reducir el
coste de renderizado. Respeta movimiento reducido y colores forzados.

## Cupertino Glass

```ts
import { createPlatformTheme } from '@/platform/adapters/vue/themes'
import { createCupertinoGlassLookAndFeel } from '@/platform/adapters/vue/themes/cupertinoGlass'

const vuetify = createPlatformTheme({
  blueprint: createCupertinoGlassLookAndFeel(),
  theme: { defaultTheme: 'system' },
})
```

Paleta neutra blanca/gris en claro y grafito en oscuro, con acciones azules sólidas.
Sin degradados, halos de color ni fondos multicolor. Usa Manrope Variable con fallback a fuentes del sistema, tarjetas de 20px, controles de 12px y botones
de 38px. El vidrio combina transparencia, desenfoque, bordes finos y sombras neutras.
Los menús y diálogos conservan su material al teletransportarse.

`cupertinoGlass.css` se importa automáticamente y está acotado a este perfil.
Las variables `cupertino-glass-opacity`, `cupertino-glass-overlay-opacity`,
`cupertino-glass-highlight` y `cupertino-glass-shadow` ajustan el material por modo.
El token `header` personaliza appbar, drawer y main por igual; `input` personaliza los campos.
Sin soporte de desenfoque o con transparencia reducida usa superficies sólidas.
En móvil se reduce el desenfoque a 12px conservando la transparencia.
Las superficies VSheet también reciben vidrio; evita `color="surface"` si quieres
conservar el material en lugar de un fondo sólido explícito. El fondo es uniforme, sin figuras decorativas. Drawer, appbar y main se dibujan
como paneles independientes de 16px de radio, con 4px de margen interior en su
espacio de layout (8px entre paneles vecinos). Las cards usan opacidad de 0.12
en claro y 0.16 en oscuro; las hojas usan 0.08.
Los campos usan superficies translúcidas sin subrayado y las pestañas usan el
modo segmentado nativo de Vuetify.

Cupertino Glass usa densidad compacta, padding de tarjetas de 16px, drawer de
248px y appbar de 72px con densidad normal. El main conserva 12px de espacio interior dentro del panel.

El fondo claro usa gris mineral `#D9DDE2`. Appbar, drawer y main comparten el
color `header` y la opacidad `cupertino-glass-chrome-opacity` en ambos modos; las cards mantienen su transparencia. El fondo sigue siendo uniforme.

## Tipografía e iconos de Cupertino Glass

La factory carga Manrope Variable (incluye caracteres latinos y acentos) desde el paquete
local `@fontsource-variable/manrope`, sin solicitudes a Google Fonts.
Phosphor se integra mediante `icons.sets`, `defaultSet` y los aliases nativos de
Vuetify. Se importan solamente los componentes utilizados, sin registro global.
Los nombres `mdi-*` existentes con equivalencia usan Phosphor; el resto conserva
MDI como fallback. `mdi:mdi-home` permite solicitar explícitamente el icono MDI.
Los otros perfiles conservan su set de iconos original. La tipografía también
se aplica a menús, diálogos y tooltips teletransportados.

## JetBrains

El perfil conserva su apariencia original: acentos violetas/azules, títulos y
cabeceras monoespaciados, botones de 30px, controles y filas de 32px y appbar de
48px. Se importa desde `@/platform/adapters/vue/themes/jetbrains` y mantiene
su CSS aislado. JetBrains Glass conserva una base independiente.
