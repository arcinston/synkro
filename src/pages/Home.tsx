import { useState, useEffect } from 'react';
import { readDir } from '@tauri-apps/plugin-fs';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { toast } from 'sonner';
import { load } from '@tauri-apps/plugin-store';
import type { Store } from '@tauri-apps/plugin-store';
import { invoke } from '@tauri-apps/api/core';

const username = 'FastSync User'; // Replace this with a dynamic way to fetch/store the username in the future
type TContent = { name: string; type: 'file' | 'directory' };

const Home: React.FC = () => {
  const [syncFolderPath, setSyncFolderPath] = useState<string | null>(null);
  const [directoryContents, setDirectoryContents] = useState<TContent[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [store, setStore] = useState<Store | null>(null);
  const [initialLoadComplete, setInitialLoadComplete] = useState(false);
  const [autoSync, setAutoSync] = useState<boolean>(false);

  useEffect(() => {
    const initializeStore = async () => {
      try {
        const newStore = await load('store.json', { autoSave: false });
        setStore(newStore);

        const savedPath = await newStore.get<string>('sync-folder-path');
        setSyncFolderPath(savedPath || null);
        setAutoSync((await newStore.get<boolean>('auto-sync')) || false);
      } catch (error) {
        console.error('Error initializing store:', error);
        toast.error('Error Initializing Store', {
          description: 'Failed to initialize storage.',
        });
      } finally {
        setInitialLoadComplete(true); // Mark that the initial load is complete
      }
    };

    initializeStore();
  }, []);

  useEffect(() => {
    const checkIrohLoaded = async () => {
      try {
        const response = await invoke('get_node_info');
        console.info(response);

        toast.info('Iroh is loaded', {
          description: `Iroh is ready to use. ${response}`,
        });
      } catch (err) {
        console.error('Error checking Iroh status:', err);
        toast.error('Error Checking Iroh Status', {
          description: 'Failed to check Iroh status.',
        });
      }
    };

    checkIrohLoaded();
  }, []);

  useEffect(() => {
    const loadDirectoryContents = async () => {
      if (!syncFolderPath) return;

      setIsLoading(true);
      try {
        const entries = await readDir(syncFolderPath);

        const contents: TContent[] = entries.map((entry) => ({
          name: entry.name || 'Unknown',
          type: entry.isDirectory ? 'directory' : 'file',
        }));

        setDirectoryContents(contents);
      } catch (error) {
        console.error('Error reading directory:', error);
        toast.error('Error Reading Directory', {
          description: 'Could not read contents of the synchronized folder.',
        });
        setDirectoryContents([]); // Clear contents to avoid displaying stale data
      } finally {
        setIsLoading(false);
      }
    };

    if (syncFolderPath) {
      loadDirectoryContents();
    }
  }, [syncFolderPath]);

  const toggleAutoSync = async () => {
    const newAutoSyncValue = !autoSync;
    setAutoSync(newAutoSyncValue);

    if (store) {
      try {
        await store.set('auto-sync', newAutoSyncValue);
        await store.save();
      } catch (error) {
        console.error('Error saving auto-sync preference:', error);
        toast.error('Error Saving Preference', {
          description: 'Could not save auto-sync preference.',
        });
      }
    } else {
      console.error('Store not initialized yet.');
      toast.error('Store Error', {
        description: 'Could not save the auto-sync value.',
      });
    }
  };

  if (!initialLoadComplete) {
    return <div>Loading...</div>; // Or any other loading indicator
  }

  return (
    <div className="flex items-center justify-center min-h-screen bg-background p-4">
      <Card className="w-full max-w-2xl">
        <CardHeader>
          <CardTitle>Welcome, {username}!</CardTitle>
          <CardDescription>
            Your files are being synchronized in the background.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <h3 className="text-lg font-semibold">Sync Folder:</h3>
            <p className="text-muted-foreground">
              {syncFolderPath || 'No folder selected.'}
            </p>
          </div>
          <div>
            <h3 className="text-lg font-semibold">Directory Contents:</h3>
            {isLoading ? (
              <p className="text-muted-foreground">Loading...</p>
            ) : directoryContents.length > 0 ? (
              <ul className="list-disc list-inside">
                {directoryContents.map((item, index) => (
                  <li key={`dir-item-${index + item.name}`}>
                    {item.name} ({item.type})
                  </li>
                ))}
              </ul>
            ) : (
              <p className="text-muted-foreground">
                No files or folders found in the selected directory.
              </p>
            )}
          </div>
          <div className="space-y-2">
            <h3 className="text-lg font-semibold">Auto-Sync</h3>
            <p className="text-muted-foreground">
              Enable or disable automatic synchronization.
            </p>
            <Button variant="outline" onClick={toggleAutoSync}>
              {autoSync ? 'Disable Auto-Sync' : 'Enable Auto-Sync'}
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
};

export default Home;
