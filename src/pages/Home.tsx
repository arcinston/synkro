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
import { listen } from '@tauri-apps/api/event';
import { useFsEvents } from '@/hooks/useFsEvents';

const username = 'FastSync User'; // Replace this with a dynamic way to fetch/store the username in the future
type TContent = { name: string; type: 'file' | 'directory' };

const Home: React.FC = () => {
  const [syncFolderPath, setSyncFolderPath] = useState<string | null>(null);
  const [directoryContents, setDirectoryContents] = useState<TContent[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [store, setStore] = useState<Store | null>(null);
  const [initialLoadComplete, setInitialLoadComplete] = useState(false);
  const [autoSync, setAutoSync] = useState<boolean>(false);
  const [gossipTicket, setGossipTicket] = useState<string | null>(null);
  const [clipboardSharingEnabled, setClipboardSharingEnabled] = useState<boolean>(false); // Added
  const [isLoadingClipboardState, setIsLoadingClipboardState] = useState<boolean>(true); // Added

  useFsEvents();

  useEffect(() => {
    const unlistenGossipReady = listen<void>('gossip-ready', (event) => {
      console.info('Gossip network is ready:', event);
      toast.success('Sync Network Ready', {
        description: 'Successfully connected to the sync network.',
      });
    });

    return () => {
      unlistenGossipReady.then((unlistenFn) => unlistenFn());
    };
  }, []);

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

  // Effect to fetch initial clipboard sharing state
  useEffect(() => {
    const fetchClipboardState = async () => {
      setIsLoadingClipboardState(true);
      try {
        const enabled = await invoke<boolean>('is_clipboard_sharing_enabled');
        setClipboardSharingEnabled(enabled);
      } catch (error) {
        console.error('Error fetching clipboard sharing state:', error);
        toast.error('Failed to load clipboard state');
      } finally {
        setIsLoadingClipboardState(false);
      }
    };

    if (initialLoadComplete) { // Ensure store and other initial setup is done
      fetchClipboardState();
    }
  }, [initialLoadComplete]);

  useEffect(() => {
    const checkIrohLoaded = async () => {
      try {
        const response = await invoke('get_node_info');
        console.info(response);

        toast.info('Iroh is loaded', {
          description: `Iroh is ready to use. ${response as string}`, // Added as string
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

  const toggleClipboardSharing = async () => {
    const newState = !clipboardSharingEnabled;
    try {
      if (newState) {
        await invoke('enable_clipboard_sharing');
        toast.success('Universal Clipboard Enabled');
      } else {
        await invoke('disable_clipboard_sharing');
        toast.info('Universal Clipboard Disabled');
      }
      setClipboardSharingEnabled(newState);
    } catch (error) {
      console.error('Error toggling clipboard sharing:', error);
      toast.error('Failed to update clipboard sharing preference');
    }
  };

  const handleCreateAndCopyGossipTicket = async () => {
    try {
      const ticket = await invoke<string>('create_gossip_ticket');
      setGossipTicket(ticket);
      await navigator.clipboard.writeText(ticket);
      toast.success('Gossip Ticket Copied!', {
        description: 'The gossip ticket has been copied to your clipboard.',
      });
    } catch (error) {
      console.error('Error creating/copying gossip ticket:', error);
      toast.error('Failed to Create Gossip Ticket', {
        description:
          typeof error === 'string'
            ? error
            : 'Could not create or copy the gossip ticket.',
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
          <div className="space-y-2">
            <h3 className="text-lg font-semibold">Share Sync Session</h3>
            <p className="text-muted-foreground">
              Create a new gossip ticket to invite others to this sync session.
            </p>
            <Button variant="default" onClick={handleCreateAndCopyGossipTicket}>
              Create & Copy Gossip Ticket
            </Button>
            {gossipTicket && (
              <p className="text-sm text-muted-foreground break-all mt-2">
                Last ticket: {gossipTicket}
              </p>
            )}
          </div>

          {/* Universal Clipboard Section */}
          <div className="space-y-2">
            <h3 className="text-lg font-semibold">Universal Clipboard</h3>
            <p className="text-muted-foreground">
              Share your clipboard content across connected devices.
            </p>
            {isLoadingClipboardState ? (
              <p className="text-muted-foreground">Loading clipboard setting...</p>
            ) : (
              <Button variant="outline" onClick={toggleClipboardSharing}>
                {clipboardSharingEnabled ? 'Disable Universal Clipboard' : 'Enable Universal Clipboard'}
              </Button>
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  );
};

export default Home;
