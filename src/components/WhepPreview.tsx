import { useEffect, useRef, useState } from "react";
import { connectWhep } from "../lib/whep";

interface Props {
  endpoint: string;
  active: boolean;
  onConnected: () => void;
  onFailure: (reason: string) => void;
  t: (key: string) => string;
}

export function WhepPreview({ endpoint, active, onConnected, onFailure, t }: Props) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const [connecting, setConnecting] = useState(false);

  useEffect(() => {
    if (!active) {
      if (videoRef.current) videoRef.current.srcObject = null;
      return;
    }
    const controller = new AbortController();
    let close: (() => Promise<void>) | undefined;
    setConnecting(true);
    connectWhep(
      endpoint,
      (stream) => {
        if (videoRef.current) {
          videoRef.current.srcObject = stream;
          void videoRef.current.play();
        }
      },
      controller.signal,
    )
      .then((session) => {
        close = session.close;
        setConnecting(false);
        onConnected();
      })
      .catch((error: unknown) => {
        if (controller.signal.aborted) return;
        setConnecting(false);
        onFailure(error instanceof Error ? error.message : String(error));
      });
    return () => {
      controller.abort();
      if (close) void close();
    };
  }, [active, endpoint, onConnected, onFailure]);

  return (
    <div className="preview-stage">
      <video ref={videoRef} autoPlay muted playsInline />
      {connecting && <div className="preview-overlay">{t("preview.negotiating")}</div>}
      {!active && <div className="preview-overlay">{t("preview.waiting")}</div>}
    </div>
  );
}
