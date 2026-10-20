# Texto del programa HITO, en español de Argentina.
# Todos los idiomas tienen los mismos mensajes. La estructura sigue a Revit
# (ADR 0007) y los términos, el uso de la ingeniería argentina (ADR 0008).

app-title = HITO

## Fichas y grupos de la cinta

ribbon-tab-structure = Estructura
ribbon-tab-view = Vista
ribbon-tab-manage = Gestionar
ribbon-group-structure = Estructura
ribbon-group-foundation = Fundaciones
ribbon-group-datum = Referencia
ribbon-group-create = Crear
ribbon-group-windows = Ventanas
ribbon-group-settings = Configuración

## Comandos

command-beam = Viga
command-wall = Muro
command-column = Columna
command-slab = Losa
command-isolated-footing = Base aislada
command-level = Nivel
command-grid = Eje
command-materials = Materiales
command-project-units = Unidades de proyecto
command-unavailable = Todavía no disponible
# La ayuda de un botón de la cinta, con el atajo del comando, como en Revit.
command-tooltip = { $command } ({ $alias })

## La línea de comandos, como en AutoCAD

command-line-label = Comando
command-line-hint = Escribir un comando
command-unknown = Comando desconocido: { $input }
command-not-available = { $command } todavía no está disponible
# El mensaje de AutoCAD cuando Esc cancela un comando.
command-cancelled = *Cancelar*
command-failed = No se pudo completar { $command }
input-empty = Escribir una respuesta, o Esc para cancelar
# La línea de comandos lee los números como AutoCAD, en todos los idiomas:
# el punto separa los decimales y la coma, las coordenadas.
input-not-a-point = Ingresar un punto como x,y o x,y,z, por ejemplo 3.5,2
input-not-a-value = Ingresar un valor, como 3.5 o 350 cm

## Niveles

# Los niveles nuevos se llaman con esta palabra y un número, como "Nivel 3".
# Los nombres son datos: no cambian al cambiar el idioma.
level-name = Nivel
level-prompt-elevation = Ingresar la elevación del nuevo nivel, como 3.5 o 350 cm
level-name-empty = El nivel necesita un nombre
level-name-taken = Ya hay otro nivel llamado { $name }

# La herramienta Muro (WA). Los puntos se marcan en una vista en planta o se escriben.
wall-prompt-start = Indicar el punto inicial del muro, como 0,0
wall-prompt-next = Indicar el punto siguiente, como 5,0, @5,0 o @5<90. Esc termina la cadena
wall-same-point = El punto siguiente tiene que ser distinto del anterior
# El tipo de muro con el que empieza todo proyecto nuevo. Desde ahí es un dato.
wall-type-generic = Genérico - 200 mm
# El hormigón genérico de todo proyecto nuevo, sin resistencia: los hormigones con grado vienen de los paquetes regionales. Desde ahí es un dato.
material-concrete-generic = Hormigón

## Ejes

grid-prompt-start = Ingresar el punto inicial del eje, como 0,0
grid-prompt-end = Ingresar el punto final del eje, como 0,10
grid-points-differ = El punto final debe ser distinto del inicial
grid-name-empty = El eje necesita un rótulo
grid-name-taken = Ya hay otro eje con el rótulo { $name }

## Columnas

# La herramienta Columna (CL). En el cruce de dos ejes, la columna sigue a los ejes.
column-prompt-point = Indicar el centro de la columna, como 0,0. Esc termina
# El tipo de columna con el que empieza todo proyecto nuevo. Desde ahí es un dato.
column-type-generic = Rectangular - 300 x 300 mm

## Acciones de ventanas

action-new-3d-view = Vista 3D
action-close-inactive-views = Cerrar inactivas
action-tab-views = Vistas en fichas
action-tile-views = Vistas en mosaico
action-user-interface = Interfaz de usuario
ui-theme = Tema
theme-light = Claro
theme-dark = Oscuro
theme-system = Según el sistema
ui-navigation = Navegación 3D
navigation-wheel-orbits = La rueda orbita, Mayús + rueda desplaza
navigation-revit = Revit: la rueda desplaza, Mayús + rueda orbita
action-zoom-to-fit = Zoom para ajustar

## Paneles

panel-project-browser = Navegador de proyectos
panel-properties = Propiedades

browser-views = Vistas (todas)
browser-floor-plans = Planos de planta
browser-3d-views = Vistas 3D
browser-levels = Niveles
browser-grids = Ejes
browser-element-types = Tipos de elemento
browser-none = (ninguno)

properties-no-view = No hay ninguna vista abierta
properties-view-name = Nombre de vista
# Varios elementos seleccionados de distintas clases, como los muestra Revit.
properties-common = Comunes ({ $count })
properties-kind-level = Nivel
properties-kind-grid = Eje
properties-label = Rótulo
properties-name = Nombre
properties-elevation = Elevación
properties-edit-failed = No se pudo hacer el cambio

## Eliminar elementos

action-delete = Eliminar
action-cancel = Cancelar
# Revit avisa antes de eliminar elementos de los que dependen otros.
delete-also = { $count ->
    [one] Al eliminar { $name } también se elimina 1 elemento asociado.
   *[other] Al eliminar { $name } también se eliminan { $count } elementos asociados.
}
# El mismo aviso con varios elementos seleccionados.
delete-also-selected = { $count ->
    [one] Al eliminar los { $selected } elementos seleccionados también se elimina 1 elemento asociado.
   *[other] Al eliminar los { $selected } elementos seleccionados también se eliminan { $count } elementos asociados.
}

## Vistas

view-kind-3d = Vista 3D
view-kind-floor-plan = Plano de planta
# El nombre de Revit para la vista 3D predeterminada, igual en todos los idiomas.
view-default-3d = {"{"}3D{"}"}
view-3d-numbered = Vista 3D { $number }
viewport-no-gpu = La vista 3D necesita un dispositivo gráfico
# El cubo de vistas, como el ViewCube de Revit.
view-cube-top = SUPERIOR
view-cube-bottom = INFERIOR
view-cube-front = FRONTAL
view-cube-back = POSTERIOR
view-cube-left = IZQUIERDA
view-cube-right = DERECHA
view-cube-home = Inicio
view-cube-go-home = Ir a inicio
view-perspective = Perspectiva
view-orthographic = Ortogonal
