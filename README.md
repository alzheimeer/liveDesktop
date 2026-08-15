# Traductor Desktop

Aplicación de escritorio para traducción de voz en tiempo real utilizando la API Gemini Live de Google.

## Características

- **Traducción de voz en tiempo real**: Captura audio del sistema y lo traduce instantáneamente
- **Soporte multi-idioma**: Español, Inglés, Francés, Alemán, Italiano, Portugués, Japonés, Coreano, Chino
- **Integración con videoconferencias**: Compatible con Zoom, Google Meet, Teams y otras apps via VB-Cable
- **Interfaz moderna**: React 19 + Tailwind CSS 4
- **Multiplataforma**: Windows 10/11 y macOS 14+ (Sonoma)

## Arquitectura

```
┌─────────────────────────────────────────────────────────────┐
│                    Frontend (React + Vite)                   │
├─────────────────────────────────────────────────────────────┤
│                    Tauri 2.x Bridge                          │
├─────────────────────────────────────────────────────────────┤
│                    Backend (Rust)                            │
│  ┌─────────────┬─────────────┬─────────────┬─────────────┐  │
│  │   Audio     │    Auth     │   Gemini    │   Storage   │  │
│  │  WASAPI     │  BetterAuth │   Live WS   │   SQLite    │  │
│  │  VB-Cable   │  Keyring    │   Streaming │   Config    │  │
│  └─────────────┴─────────────┴─────────────┴─────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## Requisitos

### Windows
- Windows 10/11 (64-bit)
- [WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) (incluido en Windows 11)
- [VB-Cable](https://vb-audio.com/Cable/) (opcional, para inyección de audio)

### macOS
- macOS 14.0 Sonoma o superior
- Permisos de grabación de pantalla (System Settings > Privacy & Security > Screen Recording)

## Instalación

### Desde binarios (recomendado)
1. Descarga el instalador desde [Releases](https://github.com/your-org/traductor-desktop/releases)
2. Windows: Ejecuta el instalador `.exe` (NSIS)
3. macOS: Abre el `.dmg` y arrastra la app a Aplicaciones

### Desde código fuente

```bash
# Clonar repositorio
git clone https://github.com/your-org/traductor-desktop.git
cd traductor-desktop

# Instalar dependencias de Node.js
npm install

# Desarrollo
npm run tauri dev

# Build de producción
npm run tauri build
```

## Uso

### Configuración inicial

1. **Autenticación**: Inicia sesión con tu cuenta (requiere API key de Gemini)
2. **Seleccionar idiomas**: Configura idioma origen y destino
3. **Seleccionar dispositivo de audio**: Elige el micrófono o fuente de audio del sistema

### Traducción en tiempo real

1. Click en "Iniciar traducción"
2. El audio capturado se traduce automáticamente
3. El texto traducido aparece en la interfaz
4. Opcionalmente, el audio traducido se reproduce o inyecta en VB-Cable

### Integración con Zoom/Meet/Teams

1. Instala [VB-Cable](https://vb-audio.com/Cable/)
2. En Traductor Desktop: Configura VB-Cable como salida de audio
3. En tu app de videoconferencia: Selecciona "CABLE Output" como micrófono
4. Los participantes escucharán tu voz traducida

## Desarrollo

### Estructura del proyecto

```
traductor-desktop/
├── src/                    # Frontend React
│   ├── components/         # Componentes UI
│   ├── hooks/              # Custom hooks
│   └── stores/             # Estado global (Zustand)
├── src-tauri/              # Backend Rust
│   ├── src/
│   │   ├── audio/          # Captura y procesamiento de audio
│   │   ├── auth/           # Autenticación y tokens
│   │   ├── billing/        # Facturación y suscripciones
│   │   ├── commands/       # Comandos Tauri
│   │   ├── gemini/         # Cliente WebSocket Gemini Live
│   │   ├── storage/        # Base de datos y configuración
│   │   ├── tray/           # System tray
│   │   └── updater/        # Auto-actualizaciones
│   └── Cargo.toml
├── package.json
└── tauri.conf.json
```

### Comandos útiles

```bash
# Desarrollo con hot-reload
npm run tauri dev

# Compilar solo backend Rust
cd src-tauri && cargo build --release

# Ejecutar tests
cd src-tauri && cargo test

# Verificar código
cd src-tauri && cargo check

# Linting frontend
npm run lint

# Build de producción
npm run tauri build
```

### Pruebas en macOS

Para probar la aplicación en macOS localmente:
1. Asegúrate de tener los prerrequisitos de compilación instalados (Xcode Command Line Tools): `xcode-select --install`.
2. Otorga los permisos necesarios para la captura de pantalla y micrófono en **Configuración del Sistema > Privacidad y seguridad**. Al correr la app en modo dev, la terminal que utilices (ej. VS Code, iTerm, Terminal) necesitará estos permisos.
3. Para compilar y correr la aplicación en modo desarrollo: `npm run tauri dev`.
4. Si tienes problemas con variables de entorno, asegúrate de configurar tu archivo `.env.local` con las credenciales necesarias antes de iniciar la app.

### Tests

```bash
# Tests unitarios (Rust)
cd src-tauri && cargo test

# Doc-tests
cd src-tauri && cargo test --doc

# Tests con proptest (property-based testing)
cd src-tauri && cargo test --features proptest
```

## Tecnologías

### Backend (Rust)
- **Tauri 2.x** - Framework de aplicaciones de escritorio
- **tokio** - Runtime async
- **tokio-tungstenite** - WebSocket client para Gemini Live
- **windows-rs** - APIs de Windows (WASAPI)
- **rusqlite** - Base de datos SQLite
- **keyring** - Almacenamiento seguro de credenciales
- **rubato** - Resampling de audio

### Frontend
- **React 19** - UI framework
- **Vite** - Build tool
- **Tailwind CSS 4** - Estilos
- **TypeScript** - Type safety

## Seguridad

- Las credenciales se almacenan en el keyring del sistema operativo
- La comunicación con Gemini usa WebSocket sobre TLS
- Los tokens de sesión tienen expiración de 1 hora
- La base de datos local puede cifrarse opcionalmente

## Licencia

MIT License - ver [LICENSE](LICENSE) para detalles.

## Contribuir

1. Fork el repositorio
2. Crea una rama para tu feature (`git checkout -b feature/nueva-funcionalidad`)
3. Commit tus cambios (`git commit -am 'Añadir nueva funcionalidad'`)
4. Push a la rama (`git push origin feature/nueva-funcionalidad`)
5. Abre un Pull Request

## Soporte

- [Issues](https://github.com/your-org/traductor-desktop/issues) - Reportar bugs
- [Discussions](https://github.com/your-org/traductor-desktop/discussions) - Preguntas y sugerencias
