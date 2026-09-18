export interface WhepSession {
  peer: RTCPeerConnection;
  close: () => Promise<void>;
}

export async function connectWhep(
  endpoint: string,
  onTrack: (stream: MediaStream) => void,
  signal: AbortSignal,
): Promise<WhepSession> {
  const peer = new RTCPeerConnection({ bundlePolicy: "max-bundle" });
  peer.addTransceiver("video", { direction: "recvonly" });
  peer.addTransceiver("audio", { direction: "recvonly" });
  const stream = new MediaStream();
  peer.ontrack = (event) => {
    stream.addTrack(event.track);
    onTrack(stream);
  };

  try {
    const offer = await peer.createOffer();
    await peer.setLocalDescription(offer);
    await waitForIceGathering(peer, signal);
    if (!peer.localDescription?.sdp) {
      throw new Error("WHEP offer SDP was not generated");
    }
    const response = await fetch(endpoint, {
      method: "POST",
      headers: {
        Accept: "application/sdp",
        "Content-Type": "application/sdp",
      },
      body: peer.localDescription.sdp,
      signal,
    });
    if (!response.ok) {
      const detail = (await response.text()).trim();
      throw new Error(`WHEP ${response.status}: ${detail || response.statusText}`);
    }
    const answer = await response.text();
    await peer.setRemoteDescription({ type: "answer", sdp: answer });
    const resource = response.headers.get("location");
    return {
      peer,
      close: async () => {
        peer.close();
        if (resource) {
          const url = new URL(resource, endpoint).toString();
          await fetch(url, { method: "DELETE" }).catch(() => undefined);
        }
      },
    };
  } catch (error) {
    peer.close();
    throw error;
  }
}

function waitForIceGathering(peer: RTCPeerConnection, signal: AbortSignal) {
  if (peer.iceGatheringState === "complete") return Promise.resolve();
  return new Promise<void>((resolve, reject) => {
    const timeout = window.setTimeout(() => finish(), 4_000);
    const finish = () => {
      window.clearTimeout(timeout);
      peer.removeEventListener("icegatheringstatechange", handleState);
      signal.removeEventListener("abort", handleAbort);
      resolve();
    };
    const handleState = () => {
      if (peer.iceGatheringState === "complete") finish();
    };
    const handleAbort = () => {
      finish();
      reject(new DOMException("WHEP connection aborted", "AbortError"));
    };
    peer.addEventListener("icegatheringstatechange", handleState);
    signal.addEventListener("abort", handleAbort, { once: true });
  });
}
