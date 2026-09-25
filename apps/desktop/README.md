# OpenCode Conductor

Aplicación de escritorio local basada en Tauri 2, React/TypeScript y Rust.

## Desarrollo

Desde `apps/desktop`:

```powershell
npm install
npm run tauri dev
```

Para probar la conexión inicial, inicia un servidor OpenCode en otra terminal:

```powershell
opencode serve --hostname 127.0.0.1 --port 4096
```

OpenCode v2 muestra una contraseña temporal del servidor en la salida de `serve`. En la aplicación, usa `http://127.0.0.1:4096`, el usuario `opencode` y esa contraseña. Esta primera iteración valida la interfaz Tauri y la consulta Rust autenticada al endpoint local de información de OpenCode.

## Comprobaciones

```powershell
npm run typecheck
npm run build
cd src-tauri
cargo test
```

La GUI usa IPC de Tauri. No se carga contenido externo ni se expone el backend a la red.
