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

## Paneles

panel-project-browser = Navegador de proyectos
panel-properties = Propiedades

browser-views = Vistas (todas)
browser-3d-views = Vistas 3D
browser-levels = Niveles
browser-element-types = Tipos de elemento
browser-none = (ninguno)

properties-no-view = No hay ninguna vista abierta
properties-view-name = Nombre de vista

## Vistas

view-kind-3d = Vista 3D
# El nombre de Revit para la vista 3D predeterminada, igual en todos los idiomas.
view-default-3d = {"{"}3D{"}"}
view-3d-numbered = Vista 3D { $number }
viewport-no-gpu = La vista 3D necesita un dispositivo gráfico
