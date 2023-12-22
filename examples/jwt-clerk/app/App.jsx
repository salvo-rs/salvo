import { useCallback, useState } from "react";
import logo from "./logo.svg";
import "./styles/App.css";
import { useSession, useAuth } from "@clerk/clerk-react";

function App() {
  const { isLoaded, session } = useSession();
  const [content, setContent] = useState("");
  const { getToken } = useAuth();
  const getWelcome = useCallback(async () => {
    const res = await fetch('welcome', {
      headers: { Authorization: `Bearer ${await getToken()}` }
    });
    let text = await res.text();
    console.log(text);
    setContent(text);
  });
  return (
    <div className="app">
      <button onClick={getWelcome}>Get data from server</button>
      <p>{content}</p>
      <img src={logo} alt="logo" />
      <a
        href="https://docs.clerk.dev/reference/clerk-react"
        target="_blank"
        rel="noopener noreferrer"
      >
      </a>
    </div>
  );
}

export default App;
