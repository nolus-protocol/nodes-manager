# Nodes Manager UI

Modern Vue 3 frontend for the Nodes Manager infrastructure console, built with TypeScript, Vite, and the Nolus Web Components library.

## Tech Stack

- **Vue 3** - Progressive JavaScript framework
- **TypeScript** - Type-safe development
- **Vite** - Next-generation build tool with HMR
- **Nolus Web Components** - Reusable UI component library from `web-components` npm package

## Available Components from Nolus Library

### Atoms
- `AssetItem`, `Badge`, `Button`, `Checkbox`, `Dropdown`, `FormControl`, `HelpText`, `Input`, `Label`, `Radio`, `Spinner`, `SvgIcon`, `Toggle`, `Tooltip`, `Asset`

### Molecules
- `AdvancedFormControl`, `Alert`, `Dialog`, `Popover`, `Proposal`, `ProposalVotingLine`, `Slider`, `Stepper`, `Toast`, `Widget`

### Organisms
- `Lease`, `Table`, `TableRow`

## Development

```bash
# Install dependencies
npm install

# Start development server with hot-reload (proxies API to localhost:3000)
npm run dev

# Type-check without building
npm run type-check

# Build for production
npm run build

# Preview production build locally
npm run preview
```

## Project Structure

```
ui/
├── src/
│   ├── assets/           # Static assets (images, fonts, etc.)
│   ├── components/       # Vue components
│   ├── services/         # API service layer
│   │   └── api.ts       # Backend API client
│   ├── types/           # TypeScript type definitions
│   │   └── api.ts       # API response types matching Rust backend
│   ├── views/           # Page-level components
│   │   └── DashboardView.vue  # Main dashboard
│   ├── App.vue          # Root component
│   ├── main.ts          # Application entry point
│   └── style.css        # Global styles
├── index.html           # HTML entry point
└── README.md           # This file
```

## API Integration

The frontend communicates with the Rust backend via the `/api` endpoint. During development, Vite proxies API requests to `http://localhost:3000`. In production, the Rust backend serves both the API and the built frontend from the `dist-ui` directory.

### API Endpoints Used

- `GET /api/health/nodes` - Get all node health statuses
- `GET /api/health/hermes` - Get all Hermes relayer statuses
- `GET /api/health/etl` - Get all ETL service statuses
- `GET /api/config/nodes` - Get node configurations
- `GET /api/config/hermes` - Get Hermes configurations
- `GET /api/config/etl` - Get ETL configurations
- `POST /api/maintenance/nodes/{node_name}/restart` - Restart a node
- `POST /api/maintenance/nodes/{node_name}/prune` - Prune a node
- `POST /api/maintenance/hermes/{hermes_name}/restart` - Restart Hermes
- `POST /api/snapshots/{node_name}/create` - Create snapshot
- `POST /api/snapshots/{node_name}/restore` - Restore from snapshot
- `POST /api/state-sync/{node_name}/execute` - Execute state sync

## Features

### Dashboard Overview
- Real-time metrics for nodes, relayers, and ETL services
- Health percentage and operational status
- Server count and infrastructure overview

### Blockchain Nodes Management
- View all nodes with status, block height, and network info
- Real-time health monitoring
- Actions: Restart, Prune, Snapshot, Restore, State Sync
- Search and filter capabilities

### Hermes Relayers Management
- Monitor relayer status and uptime
- Restart relayers with one click
- View dependent nodes

### ETL Services Management
- Monitor ETL service health
- Track response times and HTTP status codes
- Restart services as needed

### Toast Notifications
- Success, error, and warning messages
- Auto-dismiss after 5 seconds

## Building for Production

The production build is automatically created when you run:

```bash
npm run build
```

This generates optimized static files in the `dist-ui/` directory at the project root, which the Rust backend serves automatically.

## Integration with Rust Backend

The Rust backend (`manager/src/web/server.rs`) is configured to:
1. Serve API routes under `/api/*`
2. Serve static assets from `/dist-ui/assets/*`
3. Fallback to `index.html` for client-side routing (SPA support)

When you build the frontend, the files are placed in `dist-ui/`, and the Rust binary includes `dist-ui/index.html` at compile time for the fallback handler.

## Development Workflow

1. Start the Rust backend: `cargo run --release`
2. In another terminal, start the Vue dev server: `npm run dev`
3. Open `http://localhost:5173` in your browser
4. Changes to Vue files will hot-reload automatically
5. API calls are proxied to the Rust backend at `localhost:3000`

## Production Deployment

1. Build the frontend: `npm run build`
2. Build the Rust backend: `cargo build --release`
3. Run the Rust binary: `./target/release/manager`
4. Access the UI at `http://localhost:3000` (or configured host/port)

The built Vue app is embedded in the Rust binary, so you only need to deploy the single `manager` executable.
