import { useState, useEffect } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

// Define constants for event types (optional but good practice)
export type FsEventType = {
  event_type: 'Modify' | 'Create' | 'Remove' | 'Error' | 'Other';
  path: string;
};

export const useFsEvents = () => {
  const [latestEvent, setLatestEvent] = useState<FsEventType | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let unlisten: UnlistenFn;

    const setupListener = async () => {
      try {
        unlisten = await listen<FsEventType>('fs-event', (event) => {
          console.info('Received fs-event:', event.payload);
          // setLatestEvent(event.payload);
        });
        setError(null);
      } catch (err) {
        console.error("Failed to set up 'fs-event' listener:", err);
        setError(`Failed to listen for filesystem events: ${err}`);
        setLatestEvent(null);
      }
    };

    setupListener();

    return () => {
      if (unlisten) {
        console.info("Cleaning up 'fs-event' listener");
        unlisten();
      }
    };
  }, []);

  return { latestEvent, error };
};
