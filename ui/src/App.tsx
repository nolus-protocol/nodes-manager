import { useState, useEffect, useCallback } from 'react';
import { Skeleton } from '@kostovster/ui';
import { Navigation } from '@/components/layout/Navigation';
import { Dashboard } from '@/pages/Dashboard';
import { NodesPage } from '@/pages/Nodes';
import { ServicesPage } from '@/pages/Services';
import {
  fetchNodeConfigs,
  fetchHermesConfigs,
  fetchNodeHealth,
  fetchHermesHealth,
  fetchEtlHealth,
} from '@/api/client';
import type { NodeConfig, NodeHealth, HermesConfig, HermesHealth, EtlHealth } from '@/types';

const REFRESH_INTERVAL = 30000;

type Page = 'dashboard' | 'nodes' | 'services';

function App() {
  const [currentPage, setCurrentPage] = useState<Page>(() => {
    const saved = localStorage.getItem('currentPage') as Page;
    return saved && ['dashboard', 'nodes', 'services'].includes(saved) ? saved : 'dashboard';
  });

  const [nodeConfigs, setNodeConfigs] = useState<Record<string, NodeConfig>>({});
  const [hermesConfigs, setHermesConfigs] = useState<Record<string, HermesConfig>>({});
  
  const [nodeHealth, setNodeHealth] = useState<NodeHealth[]>([]);
  const [hermesHealth, setHermesHealth] = useState<HermesHealth[]>([]);
  const [etlHealth, setEtlHealth] = useState<EtlHealth[]>([]);
  
  const [isLoading, setIsLoading] = useState(true);
  const [isRefreshing, setIsRefreshing] = useState(false);

  useEffect(() => {
    localStorage.setItem('currentPage', currentPage);
  }, [currentPage]);

  const loadAllData = useCallback(async (showRefreshing = false) => {
    if (showRefreshing) setIsRefreshing(true);
    
    try {
      const [nConfigs, hConfigs, nHealth, hHealth, eHealth] = await Promise.all([
        fetchNodeConfigs(),
        fetchHermesConfigs(),
        fetchNodeHealth(),
        fetchHermesHealth(),
        fetchEtlHealth(),
      ]);
      
      setNodeConfigs(nConfigs);
      setHermesConfigs(hConfigs);
      setNodeHealth(nHealth);
      setHermesHealth(hHealth);
      setEtlHealth(eHealth);
    } catch (error) {
      console.error('Failed to load data:', error);
    } finally {
      setIsLoading(false);
      setIsRefreshing(false);
    }
  }, []);

  useEffect(() => {
    loadAllData();
    
    const interval = setInterval(() => loadAllData(false), REFRESH_INTERVAL);
    
    const handleFocus = () => loadAllData(false);
    window.addEventListener('focus', handleFocus);
    
    return () => {
      clearInterval(interval);
      window.removeEventListener('focus', handleFocus);
    };
  }, [loadAllData]);

  const handleRefresh = useCallback(() => {
    loadAllData(true);
  }, [loadAllData]);

  if (isLoading) {
    return (
      <div className="min-h-screen bg-background">
        {/* Header skeleton */}
        <header className="sticky top-0 z-50 border-b bg-card/95 backdrop-blur">
          <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
            <div className="flex h-16 items-center justify-between">
              <div className="flex items-center gap-6">
                <Skeleton className="h-7 w-7 rounded" />
                <Skeleton className="h-6 w-px" />
                <div className="flex gap-2">
                  <Skeleton className="h-8 w-24 rounded-md" />
                  <Skeleton className="h-8 w-16 rounded-md" />
                  <Skeleton className="h-8 w-20 rounded-md" />
                </div>
              </div>
              <Skeleton className="h-5 w-16 rounded-full" />
            </div>
          </div>
        </header>
        
        <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-4 sm:py-6 lg:py-8">
          {/* Metrics grid skeleton */}
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6 mb-8">
            {[...Array(4)].map((_, i) => (
              <div key={i} className="border rounded-lg p-6 bg-card">
                <div className="flex justify-between items-center mb-4">
                  <Skeleton className="h-3 w-24" />
                  <Skeleton className="h-5 w-5 rounded" />
                </div>
                <Skeleton className="h-9 w-16 mb-2" />
                <Skeleton className="h-4 w-32" />
              </div>
            ))}
          </div>
          
          {/* Content skeleton */}
          <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
            <div className="lg:col-span-2 space-y-6">
              {[...Array(2)].map((_, i) => (
                <div key={i} className="border rounded-lg bg-card p-6">
                  <Skeleton className="h-6 w-40 mb-4" />
                  <div className="space-y-3">
                    {[...Array(4)].map((_, j) => (
                      <div key={j} className="flex items-center gap-3">
                        <Skeleton className="h-9 w-9 rounded-full" />
                        <div className="flex-1">
                          <Skeleton className="h-4 w-48 mb-1" />
                          <Skeleton className="h-3 w-24" />
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              ))}
            </div>
            <div className="space-y-6">
              {[...Array(2)].map((_, i) => (
                <div key={i} className="border rounded-lg bg-card p-6">
                  <Skeleton className="h-6 w-36 mb-4" />
                  <Skeleton className="h-8 w-24 mb-2" />
                  <Skeleton className="h-4 w-32" />
                </div>
              ))}
            </div>
          </div>
        </main>
      </div>
    );
  }

  const renderPage = () => {
    switch (currentPage) {
      case 'nodes':
        return (
          <NodesPage 
            nodes={nodeHealth}
            configs={nodeConfigs}
            onRefresh={handleRefresh}
            isLoading={isRefreshing}
          />
        );
      case 'services':
        return (
          <ServicesPage 
            hermes={hermesHealth}
            hermesConfigs={hermesConfigs}
            etl={etlHealth}
            onRefresh={handleRefresh}
            isLoading={isRefreshing}
          />
        );
      default:
        return (
          <Dashboard 
            nodes={nodeHealth}
            nodeConfigs={nodeConfigs}
            hermes={hermesHealth}
            etl={etlHealth}
            isLoading={isRefreshing}
            onNavigateToNodes={() => setCurrentPage('nodes')}
            onNavigateToServices={() => setCurrentPage('services')}
          />
        );
    }
  };

  return (
    <div className="min-h-screen bg-background">
      <Navigation 
        currentPage={currentPage}
        onPageChange={setCurrentPage}
      />
      <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-4 sm:py-6 lg:py-8">
        {renderPage()}
      </main>
    </div>
  );
}

export default App;
