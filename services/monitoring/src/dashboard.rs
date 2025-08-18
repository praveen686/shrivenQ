//! Real-time monitoring dashboard for ShrivenQuant

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct DashboardState {
    pub metrics: Arc<RwLock<SystemMetrics>>,
}

impl DashboardState {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(SystemMetrics::default())),
        }
    }
    
    pub async fn get_current_metrics(&self) -> SystemMetrics {
        self.metrics.read().await.clone()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub services_online: u32,
    pub total_services: u32,
    pub orders_processed: u64,
    pub error_rate: f64,
    pub latency_ms: f64,
}

pub const DASHBOARD_HTML: &str = r#"
<!DOCTYPE html>
<html>
<head>
    <title>ShrivenQuant Monitoring Dashboard</title>
    <style>
        body {
            font-family: monospace;
            background: #1a1a1a;
            color: #00ff00;
            padding: 20px;
        }
        h1 {
            color: #00ff00;
            text-align: center;
            text-shadow: 0 0 10px #00ff00;
        }
        .metrics {
            display: grid;
            grid-template-columns: repeat(3, 1fr);
            gap: 20px;
            margin-top: 30px;
        }
        .metric-card {
            background: #0a0a0a;
            border: 1px solid #00ff00;
            padding: 20px;
            border-radius: 5px;
            box-shadow: 0 0 10px rgba(0,255,0,0.3);
        }
        .metric-title {
            font-size: 14px;
            color: #888;
            margin-bottom: 10px;
        }
        .metric-value {
            font-size: 32px;
            font-weight: bold;
            color: #00ff00;
        }
        .status-grid {
            display: grid;
            grid-template-columns: repeat(4, 1fr);
            gap: 10px;
            margin-top: 30px;
        }
        .service-status {
            background: #0a0a0a;
            border: 1px solid #333;
            padding: 10px;
            text-align: center;
            border-radius: 3px;
        }
        .service-status.online {
            border-color: #00ff00;
            box-shadow: 0 0 5px rgba(0,255,0,0.5);
        }
        .service-status.offline {
            border-color: #ff0000;
            box-shadow: 0 0 5px rgba(255,0,0,0.5);
        }
        .logo {
            text-align: center;
            font-size: 20px;
            margin-bottom: 20px;
            color: #00ff00;
        }
    </style>
    <script>
        async function connectWebSocket() {
            const ws = new WebSocket('ws://localhost:50063/ws');
            
            ws.onmessage = (event) => {
                const data = JSON.parse(event.data);
                updateMetrics(data);
            };
            
            ws.onerror = (error) => {
                console.error('WebSocket error:', error);
            };
            
            ws.onclose = () => {
                setTimeout(connectWebSocket, 1000);
            };
        }
        
        function updateMetrics(data) {
            document.getElementById('services-online').textContent = data.services_online || 0;
            document.getElementById('orders-processed').textContent = data.orders_processed || 0;
            document.getElementById('error-rate').textContent = (data.error_rate || 0).toFixed(2) + '%';
            document.getElementById('latency').textContent = (data.latency_ms || 0).toFixed(2) + 'ms';
        }
        
        window.onload = connectWebSocket;
    </script>
</head>
<body>
    <div class="logo">⚡ SHRIVENQUANT TRADING SYSTEM ⚡</div>
    <h1>System Monitoring Dashboard</h1>
    
    <div class="metrics">
        <div class="metric-card">
            <div class="metric-title">SERVICES ONLINE</div>
            <div class="metric-value" id="services-online">0</div>
        </div>
        <div class="metric-card">
            <div class="metric-title">ORDERS PROCESSED</div>
            <div class="metric-value" id="orders-processed">0</div>
        </div>
        <div class="metric-card">
            <div class="metric-title">ERROR RATE</div>
            <div class="metric-value" id="error-rate">0.00%</div>
        </div>
        <div class="metric-card">
            <div class="metric-title">LATENCY</div>
            <div class="metric-value" id="latency">0.00ms</div>
        </div>
    </div>
    
    <div class="status-grid">
        <div class="service-status online">Auth Service</div>
        <div class="service-status online">Market Connector</div>
        <div class="service-status online">Risk Manager</div>
        <div class="service-status online">Execution Router</div>
        <div class="service-status online">Portfolio Manager</div>
        <div class="service-status online">Data Aggregator</div>
        <div class="service-status online">Trading Gateway</div>
        <div class="service-status online">Options Engine</div>
        <div class="service-status online">OMS</div>
        <div class="service-status online">Order Book</div>
        <div class="service-status online">Monitoring</div>
        <div class="service-status offline">ML Inference</div>
    </div>
</body>
</html>
"#;