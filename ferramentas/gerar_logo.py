#!/usr/bin/env python3
"""Gera as constantes de logo do `magi` a partir da arte de referencia do
sistema MAGI.

    python3 ferramentas/gerar_logo.py caminho/da/arte.jpg

Sai no stdout no formato que vai colado no `magi`, substituindo os blocos
existentes. Requer Pillow — que e dependencia SO daqui, nao do `magi`: o
programa carrega o desenho ja pronto e nao precisa da imagem.

Duas decisoes explicam por que essa versao parece a referencia e as
anteriores nao:

**Braille (U+2800), nao meio-bloco nem quadrante.** A celula braille carrega
2x4 subpixels, contra 1x2 do meio-bloco e 2x2 do quadrante. E a celula certa
pra line-art — e a mesma que drawille e plotext usam pra grafico de linha.
Com meio-bloco o traco fino nao cabe e o desenho vira degrau.

**Contorno, nao preenchimento.** A tentativa anterior enchia as faixas pra
elas sobreviverem a reducao, e a marca virava um bloco solido — o oposto da
referencia, que e traco fino com brilho. Aqui a silhueta solida e so um
passo intermediario: dela sai o CONTORNO, que e o que vai pra tela.

Reduzir tambem exige cuidado: media apaga linha fina, entao a reducao e por
maximo (max-pooling). Qualquer subpixel que encoste na linha acende.

O caminho completo:

1. isola o laranja da arte (a marca) e, separado, os arcos esverdeados do
   fundo, que sao boa parte da atmosfera
2. fecha os vaos da marca com dilatacao e erosao, virando area solida —
   necessario porque o texto dentro de cada faixa tambem e laranja, e sem
   fechar a reducao vira sopa
3. vaza o nucleo triangular e as divisorias entre secoes, posicionados a
   partir dos eixos medidos na propria marca
4. extrai o contorno dessa area (area menos area erodida)
5. reduz por maximo e converte em braille

Os arcos da arte cobrem so a metade de cima (de ~150 a ~30 graus): e uma
cupula, nao um circulo. O passo `completa_aneis` mede o centro e os raios
dos aneis existentes e desenha as circunferencias inteiras nesses mesmos
raios, unindo com os arcos originais — o traco irregular do original fica
onde ele existe, e o resto do circulo e fechado.
"""
import math
import sys

from PIL import Image, ImageDraw, ImageFilter

# dois enquadramentos: o amplo pega os arcos concentricos em volta (vai na
# tela de boot); o fechado e so a marca, pro canto da aba GERAL, onde os
# arcos nao caberiam sem virar sujeira
# o amplo precisa caber o anel externo inteiro (raio ~248 a partir do
# centro da arte), senao o circulo sai cortado embaixo
CROP_AMPLO = (2, 4, 506, 508)
CROP_FECHADO = (118, 74, 460, 436)

FECHAMENTO = 9          # passos de dilatacao/erosao pra solidificar as faixas
RAIO_NUCLEO = 0.30      # tamanho do triangulo central, em fracao do alcance
DIVISORIAS = (0.44, 0.62, 0.80)

BITS = {(0, 0): 0x01, (0, 1): 0x02, (0, 2): 0x04, (0, 3): 0x40,
        (1, 0): 0x08, (1, 1): 0x10, (1, 2): 0x20, (1, 3): 0x80}


def _separa(im):
    """Divide a arte em duas mascaras: o traco laranja e os arcos verdes."""
    w, h = im.size
    px = im.load()
    marca = Image.new('L', (w, h), 0)
    arcos = Image.new('L', (w, h), 0)
    mp, ap = marca.load(), arcos.load()
    for y in range(h):
        for x in range(w):
            r, g, b = px[x, y]
            if r > 95 and r > g * 1.35 and r > b * 1.8:
                mp[x, y] = 255
            elif max(r, g, b) > 34 and g >= r:
                ap[x, y] = 255
    return marca, arcos


def _centro_dos_aneis(arcos):
    """Acha o centro por busca: o certo e o que deixa o histograma de raios
    mais concentrado, porque ai cada anel cai num raio so."""
    p = arcos.load()
    w, h = arcos.size
    pts = [(x, y) for y in range(h) for x in range(w) if p[x, y]]

    def nitidez(cx, cy):
        hist = [0] * (w + h)
        for x, y in pts:
            hist[int(math.hypot(x - cx, y - cy))] += 1
        return sum(v * v for v in hist), hist

    melhor = max(((nitidez(cx, cy)[0], cx, cy)
                  for cx in range(w // 2 - 60, w // 2 + 61, 10)
                  for cy in range(h // 2 - 60, h // 2 + 61, 10)))
    _, cx, cy = melhor
    for _ in range(3):
        base = (cx, cy)
        for dx in (-4, -2, 0, 2, 4):
            for dy in (-4, -2, 0, 2, 4):
                s, _h = nitidez(base[0] + dx, base[1] + dy)
                if s > melhor[0]:
                    melhor = (s, base[0] + dx, base[1] + dy)
                    cx, cy = base[0] + dx, base[1] + dy
    return cx, cy, nitidez(cx, cy)[1]


def completa_aneis(arcos):
    """Fecha as circunferencias: a arte so tem a metade de cima."""
    cx, cy, hist = _centro_dos_aneis(arcos)
    raios = []
    for d in range(20, len(hist) - 4):
        if hist[d] > 80 and hist[d] >= max(hist[d - 3:d + 4]):
            if not raios or d - raios[-1] > 5:
                raios.append(d)
    d = ImageDraw.Draw(arcos)
    for r in raios:
        d.ellipse([cx - r, cy - r, cx + r, cy + r], outline=255, width=1)
    return arcos


def camadas(caminho, crop, com_aneis=True):
    """Separa as camadas, fecha os aneis e solidifica a marca. Tudo em
    tamanho cheio antes de recortar, pra dilatacao e circunferencia nao
    sofrerem efeito de borda. `com_aneis` desliga a busca do centro quando
    a saida nao vai usar os arcos — e a parte cara daqui."""
    im = Image.open(caminho).convert('RGB')
    marca, arcos = _separa(im)
    if com_aneis:
        arcos = completa_aneis(arcos)
    for _ in range(FECHAMENTO):
        marca = marca.filter(ImageFilter.MaxFilter(3))
    for _ in range(FECHAMENTO):
        marca = marca.filter(ImageFilter.MinFilter(3))
    return marca.crop(crop), arcos.crop(crop)


def eixos(m):
    """Centroide e as tres direcoes de braco, achadas pelo raio maximo em
    cada faixa angular. Medir em vez de assumir 0/120/240 mantem o desenho
    alinhado com a arte, que nao esta perfeitamente aprumada."""
    p = m.load()
    w, h = m.size
    sx = sy = n = 0
    for y in range(h):
        for x in range(w):
            if p[x, y]:
                sx += x
                sy += y
                n += 1
    cx, cy = sx / n, sy / n

    raio = [0.0] * 360
    for y in range(h):
        for x in range(w):
            if p[x, y]:
                d = math.hypot(x - cx, y - cy)
                a = int(math.degrees(math.atan2(y - cy, x - cx))) % 360
                raio[a] = max(raio[a], d)

    picos = []
    for a in sorted(range(360), key=lambda k: -raio[k]):
        if all(min(abs(a - b), 360 - abs(a - b)) > 60 for b in picos):
            picos.append(a)
        if len(picos) == 3:
            break
    return (cx, cy), sorted(picos), max(raio)


def line_art(marca, espessura):
    """Vaza nucleo e divisorias e devolve so o contorno do que sobrou."""
    (cx, cy), eix, alcance = eixos(marca)
    cheia = marca.copy()
    d = ImageDraw.Draw(cheia)

    pontos = []
    for i in range(3):
        meio = (eix[i] + eix[(i + 1) % 3]) / 2.0
        if abs(eix[(i + 1) % 3] - eix[i]) > 180:
            meio = (meio + 180) % 360
        r_nuc = alcance * RAIO_NUCLEO
        pontos.append((cx + r_nuc * math.cos(math.radians(meio)),
                       cy + r_nuc * math.sin(math.radians(meio))))
    d.polygon(pontos, fill=0)

    for a in eix:
        rad = math.radians(a)
        ux, uy = math.cos(rad), math.sin(rad)
        vx, vy = -uy, ux
        for frac in DIVISORIAS:
            mx, my = cx + ux * alcance * frac, cy + uy * alcance * frac
            meia = alcance * 0.5
            d.line([(mx - vx * meia, my - vy * meia),
                    (mx + vx * meia, my + vy * meia)],
                   fill=0, width=espessura * 2)

    dentro = cheia
    for _ in range(espessura):
        dentro = dentro.filter(ImageFilter.MinFilter(3))
    w, h = cheia.size
    borda = Image.new('L', (w, h), 0)
    cp, dp, bp = cheia.load(), dentro.load(), borda.load()
    for y in range(h):
        for x in range(w):
            if cp[x, y] and not dp[x, y]:
                bp[x, y] = 255
    return borda


def max_pool(m, larg, alt):
    """Reduz pegando o maximo de cada bloco. Media apagaria a linha fina;
    maximo mantem — qualquer subpixel que encoste nela acende."""
    w, h = m.size
    p = m.load()
    saida = Image.new('L', (larg, alt), 0)
    sp = saida.load()
    for ty in range(alt):
        y0 = ty * h // alt
        y1 = max(y0 + 1, (ty + 1) * h // alt)
        for tx in range(larg):
            x0 = tx * w // larg
            x1 = max(x0 + 1, (tx + 1) * w // larg)
            aceso = 0
            for y in range(y0, y1):
                for x in range(x0, x1):
                    if p[x, y]:
                        aceso = 255
                        break
                if aceso:
                    break
            sp[tx, ty] = aceso
    return saida


def braille(camada, colunas, linhas):
    """Converte em linhas de braille. Celula vazia vira espaco comum, pra
    camada de tras aparecer por baixo."""
    p = max_pool(camada, colunas * 2, linhas * 4).load()
    saida = []
    for r in range(linhas):
        celulas = []
        for c in range(colunas):
            v = 0
            for (dx, dy), bit in BITS.items():
                if p[c * 2 + dx, r * 4 + dy]:
                    v |= bit
            celulas.append(chr(0x2800 + v) if v else ' ')
        saida.append(''.join(celulas))
    return saida


def altura_para(m, colunas):
    """A celula do terminal e 1x2, entao a figura ocupa colunas*(alt/larg)/2
    linhas — independente de quantos subpixels a tecnica coloca dentro."""
    larg, alt = m.size
    return max(1, round(colunas * alt / larg / 2))


def emite(nome, linhas, comentario):
    print('%s = (   # %s' % (nome, comentario))
    for ln in linhas:
        print("    '%s'," % ln.rstrip().replace('\\', '\\\\'))
    print(')')
    print()


def main():
    if len(sys.argv) != 2:
        print('uso: gerar_logo.py caminho/da/arte.jpg', file=sys.stderr)
        return 1
    caminho = sys.argv[1]

    # boot: enquadramento amplo, com os arcos
    marca, arcos = camadas(caminho, CROP_AMPLO)
    cols = 52
    lin = altura_para(marca, cols)
    emite('LOGO_GRANDE', braille(line_art(marca, 2), cols, lin),
          '%d colunas x %d linhas' % (cols, lin))
    emite('ARCOS_GRANDE', braille(arcos, cols, lin),
          'fundo do LOGO_GRANDE, mesma grade')

    # canto: enquadramento fechado, sem arcos, traco mais fino pra nao
    # empastar no tamanho pequeno
    marca, _ = camadas(caminho, CROP_FECHADO, com_aneis=False)
    cols = 24
    lin = altura_para(marca, cols)
    emite('LOGO_PEQUENO', braille(line_art(marca, 1), cols, lin),
          '%d colunas x %d linhas' % (cols, lin))
    return 0


if __name__ == '__main__':
    sys.exit(main())
