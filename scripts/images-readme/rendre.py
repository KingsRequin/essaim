"""Refait les images du README (docs/images/).

Les aperçus sont ceux de la fenêtre des réglages (`ui/apercu.js`, calculés comme l'essaim),
rendus dans Edge sans fenêtre (protocole CDP) sur un son simulé à 120 BPM (et une montée suivie
d'un drop pour `drop.gif`), puis assemblés en GIF. Les réglages des presets fournis viennent du test `exporter_presets`.

`stream.png` est la vraie image de la fenêtre pour le stream (Direct2D, test `exporter_image_stream`),
posée sur un damier qui montre sa transparence.

`logo.gif` et `logo-nom.gif` sont le logo qui s'assemble (`logo.html`), photographié image par
image dans Edge.

    pip install websocket-client pillow
    python scripts/images-readme/rendre.py            # tout
    python scripts/images-readme/rendre.py stream     # seulement stream.png (ou : gifs, logo)
"""
import base64, copy, io, json, os, pathlib, shutil, subprocess, sys, tempfile, time, urllib.request
import websocket
from PIL import Image

ICI = pathlib.Path(__file__).resolve().parent
DEPOT = ICI.parent.parent
SORTIE = DEPOT / "docs" / "images"
EDGE = r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"
PORT = 9339
FPS = 20
N = 60  # 3 s : six temps à 120 BPM, la boucle retombe sur un kick
REFAIRE = set(sys.argv[1:]) or {"stream", "gifs", "logo"}

tmp = pathlib.Path(tempfile.mkdtemp(prefix="essaim-images-"))


def damier(cote, case=16):
    """Damier gris foncé, la convention des logiciels d'image pour « transparent »."""
    d = Image.new("RGBA", (cote, cote))
    for y in range(0, cote, case):
        for x in range(0, cote, case):
            d.paste((40, 40, 46, 255) if (x // case + y // case) % 2 else (28, 28, 32, 255), (x, y, x + case, y + case))
    return d


def ouvrir(page):
    """Ouvre `page` dans Edge sans fenêtre et attend qu'elle soit prête (`window.pret`).

    Rend le processus et de quoi lui parler : `evaluer(expression)` et `cdp(méthode, paramètres)`.
    """
    edge = subprocess.Popen([EDGE, "--headless=new", f"--remote-debugging-port={PORT}", "--allow-file-access-from-files",
                             "--force-device-scale-factor=1", f"--user-data-dir={tmp / 'edge'}", "--disable-gpu",
                             page.as_uri()])
    for _ in range(50):
        try:
            pages = json.load(urllib.request.urlopen(f"http://127.0.0.1:{PORT}/json"))
            cible = next(p for p in pages if p["type"] == "page")
            break
        except Exception:
            time.sleep(0.3)
    else:
        edge.kill()
        raise RuntimeError("Edge ne répond pas sur le port de débogage")
    ws = websocket.create_connection(cible["webSocketDebuggerUrl"], max_size=None)
    num = [0]

    def cdp(methode, params=None):
        num[0] += 1
        ws.send(json.dumps({"id": num[0], "method": methode, "params": params or {}}))
        while True:
            m = json.loads(ws.recv())
            if m.get("id") == num[0]:
                if "error" in m:
                    raise RuntimeError(m["error"])
                return m["result"]

    def evaluer(expr):
        r = cdp("Runtime.evaluate", {"expression": expr, "awaitPromise": True, "returnByValue": True})
        if "exceptionDetails" in r:
            raise RuntimeError(r["exceptionDetails"])
        return r["result"].get("value")

    while not evaluer("window.pret === true"):
        time.sleep(0.2)
    return edge, evaluer, cdp


def fermer(edge):
    edge.kill()
    edge.wait()
    time.sleep(1)  # Edge relâche ses fichiers


def gif(nom, frames, ips=FPS):
    # une palette commune tirée de quelques images, sans tramage (fond noir uni)
    h = frames[0].height
    echantillon = Image.new("RGB", (frames[0].width, h * 4))
    for i, k in enumerate(range(0, len(frames), len(frames) // 4)):
        echantillon.paste(frames[k], (0, h * i))
    pal = echantillon.quantize(colors=255, method=Image.Quantize.MEDIANCUT)
    q = [f.quantize(palette=pal, dither=Image.Dither.NONE) for f in frames]
    q[0].save(SORTIE / nom, save_all=True, append_images=q[1:], duration=1000 // ips, loop=0, optimize=True)
    print(nom, len(frames), "images", frames[0].size)


if "stream" in REFAIRE:
    subprocess.run(["cargo", "test", "--release", "exporter_image_stream", "--", "--ignored", "--quiet"],
                   cwd=DEPOT, check=True, env={**os.environ, "ESSAIM_STREAM": str(tmp / "stream.png")})
    im = Image.open(tmp / "stream.png").convert("RGBA")
    Image.alpha_composite(damier(im.width), im).convert("RGB").save(SORTIE / "stream.png", optimize=True)
    print("stream.png", im.size)

if "logo" in REFAIRE:
    # Le logo qui s'assemble, seul puis avec le nom. La pose est tenue jusqu'à 5,2 s, puis tout se
    # défait et la boucle reprend après un court noir.
    edge, evaluer, cdp = ouvrir(ICI / "logo.html")
    try:
        cote, ips = 400, 25
        for nom, avec_nom in (("logo.gif", False), ("logo-nom.gif", True)):
            duree = evaluer(f"preparer({json.dumps(avec_nom)}, {cote}, 5.2)")
            frames = []
            for i in range(round((duree + 0.3) * ips)):
                evaluer(f"rendre({i / ips})")
                photo = cdp("Page.captureScreenshot", {"format": "png", "clip": {"x": 0, "y": 0, "width": cote, "height": cote, "scale": 1}})
                frames.append(Image.open(io.BytesIO(base64.b64decode(photo["data"]))).convert("RGB"))
            gif(nom, frames, ips)
    finally:
        fermer(edge)

if "gifs" in REFAIRE:
    subprocess.run(["cargo", "test", "--release", "exporter_presets", "--", "--ignored", "--quiet"],
                   cwd=DEPOT, check=True, env={**os.environ, "ESSAIM_PRESETS": str(tmp / "presets.json")})
    presets = json.loads((tmp / "presets.json").read_text(encoding="utf-8"))

    edge, evaluer, _ = ouvrir(ICI / "rendu.html")
    try:
        def images(reglages, cote, clics=(), scenario="groove", n=N):
            urls = evaluer(f"rendre({json.dumps(reglages)}, {cote}, {n}, {FPS}, 3, {json.dumps(list(clics))}, {json.dumps(scenario)})")
            out = []
            for u in urls:
                im = Image.open(io.BytesIO(base64.b64decode(u.split(",", 1)[1]))).convert("RGBA")
                out.append(Image.alpha_composite(Image.new("RGBA", im.size, (0, 0, 0, 255)), im).convert("RGB"))
            return out

        # bandeau : quatre presets côte à côte, un clic sur le premier
        cote, noms = 300, ["Défaut", "Delta", "Ruche", "Huit"]
        series = [images(presets[n], cote, clics=(24,) if n == "Défaut" else ()) for n in noms]
        bandeau = []
        for i in range(N):
            im = Image.new("RGB", (cote * len(noms), cote))
            for j, s in enumerate(series):
                im.paste(s[i], (cote * j, 0))
            bandeau.append(im)
        gif("hero.gif", bandeau)

        # le kick (Intense), le trajet d'une image (visage souriant : quatre contours, un par anneau,
        # tirés par `contours.rs`), les effets du clic
        gif("kick.gif", images(presets["Intense"], 320))
        subprocess.run(["cargo", "test", "--release", "exporter_contours", "--", "--ignored", "--quiet"],
                       cwd=DEPOT, check=True, env={**os.environ, "ESSAIM_CONTOURS": str(tmp / "contours.json")})
        dessin = copy.deepcopy(presets["Défaut"])
        dessin["orbite"]["forme"] = "trace"
        dessin["orbite"]["contours"] = json.loads((tmp / "contours.json").read_text(encoding="utf-8"))
        dessin["orbite"]["anneaux"], dessin["orbite"]["rayon"] = 4, 34
        dessin["couleur"]["teinte"], dessin["couleur"]["vitesse_teinte"] = 290, 25
        gif("trace.gif", images(dessin, 320))
        clic = copy.deepcopy(presets["Spirale"])
        # au clic, l'onde et les éclats (déclencheurs)
        liste = clic["declencheurs"]["liste"]
        for faire in ("onde", "eclats"):
            if not any(d["quand"] == "clic" and d["faire"] == faire for d in liste):
                liste.append({**liste[0], "quand": "clic", "faire": faire, "force": 1.0, "delai": 0.0, "probabilite": 1.0})
        gif("clic.gif", images(clic, 320, clics=(4, 34)))
        # montée puis drop (Festival : palette de trois teintes, contretemps)
        gif("drop.gif", images(presets["Festival"], 320, scenario="montee", n=90))
        # constellation
        gif("constellation.gif", images(presets["Constellation"], 320))
    finally:
        fermer(edge)

shutil.rmtree(tmp, ignore_errors=True)
