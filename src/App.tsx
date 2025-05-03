import { useState } from 'react';
import './App.css';
import { Onboarding } from './pages/Onboarding';
import { Toaster } from '@/components/ui/sonner'; // Assuming you've added toast
import Home from './pages/Home';

type Page = 'onboarding' | 'home';

function App() {
  const [page, setPage] = useState<Page>('onboarding');

  return (
    <main className="container">
      {page === 'onboarding' ? (
        <Onboarding
          onComplete={() => {
            setPage('home');
          }}
        />
      ) : null}
      {page === 'home' ? <Home /> : null}
      <Toaster />
    </main>
  );
}

export default App;
