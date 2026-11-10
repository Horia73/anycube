# Plan: solver 4x4x4 cu soluții mai scurte decât TPR

Actualizat: 2026-11-10. Țintă: medie cât mai aproape de 39 de mutări (OBTM).
Cifrele măsurate sunt în `bench/RESULTS.md`.

## Unde suntem

| Mod | Medie | Timp mediu | Măsurat pe |
|---|---|---|---|
| TPR (referință) | 44,57 | 0,15 s | 1.000 de cuburi |
| `--mode fast` | 41,54 | 0,36 s | primele 100 |
| implicit | 40,60 | 2,5 s | 1.000 de cuburi |
| `--mode full` | 40,00 | 15,9 s | primele 30 |

Alte mărimi: 2x2x2 se rezolvă optim în ~2 ms; 3x3x3 cu two-phase în ~10 ms sau
optim cu `--mode full`. Pornirea durează ~25 ms.

## De ce nu am schimbat structura fazei 3

Am verificat ideea prin numărarea stărilor, înainte să scriu cod. O fază cu
`S` stări și `b` continuări utile pe mutare cere cam `ln S / ln b` mutări.

| Etapă | Stări | Continuări utile | Estimat | Măsurat |
|---|---|---|---|---|
| Faza unită (toate mutările) | 1,5×10^14 | ~22 | 10,5 | 11,1 (primul capăt) |
| Faza 3 (17 mutări permise) | 2,8×10^13 | ~12 | 12,4 | 12,4 (TPR, un singur capăt) |
| 3x3x3 | 4,3×10^19 | ~13 | 17,5 | 17,7 (optim cunoscut) |

- Suma dă ~40,6, adică exact unde suntem. Structura în trei etape e la limita ei.
- Faza 3 pierde ~2,4 mutări fiindcă lucrează cu 12 continuări în loc de 22.
- Dacă îi dau fazei 3 mai multe mutări, coordonata ei de împerechere dispare și
  spațiul crește de la 2,4×10^8 la 3,2×10^11. Faza ar prelua muncă din faza
  liberă și totalul ar ieși mai lung, nu mai scurt.
- Singura cale de a recupera cele 2,4 mutări e ca reducerea să fie o singură
  fază liberă, iar pentru ea nu există un prag de căutare suficient de bun.

## Ce am încercat după 40,67

| Idee | Rezultat |
|---|---|
| Capete și reduceri fără mutare finală redundantă | corect logic, fără câștig măsurabil |
| Tabel 3x3x3 mai puternic (53 de miliarde de clase, 1,7 GB) | căutarea exactă de ~5 ori mai rapidă |
| Căutare exactă 3x3x3 peste lot, până la 16 mutări | −0,4 la ~10 s în plus (modul full) |

## Cât de departe e 39

Pe aceleași cuburi, lotul de reduceri conține rar un 3x3x3 de 15 mutări
(2,5 la mie din stări) și destul de des unul de 16 (3%). Căutarea exactă îl
găsește pe cel de 16, dar o întrebare „există soluție în 16?" costă ~2 s.

- **~40,2** e atins de modul full, în ~14 s.
- **~39,8** ar cere loturi de 10 ori mai mari sau o căutare exactă de 10 ori
  mai rapidă. Fiecare factor de 13 în viteză aduce cam o treime de mutare.
- **39** cere o idee nouă. Nu o am încă și nu vreau să promit.

Idei rămase, în ordinea în care le-aș încerca:

1. Prag de căutare 3x3x3 pe starea inversă (maximul din 6 tabele în loc de 3):
   de 2–3 ori mai puține noduri.
2. Pruning pe muchii în faza unită: un nivel în plus în același timp, ~−0,2.
3. Tabel de împerechere pentru 4–5 perechi sub toate mutările, ca faza liberă
   să poată cere „k perechi deja făcute": −0,5 estimat, muncă multă.

## Viteză

Făcut pe 2026-11-10:

| Pas | Efect măsurat |
|---|---|
| Tabelele mici salvate pe disc | pornire din ~540 ms în ~25 ms |
| Faza 1 a 3x3x3 verificată cu tabelul exact | etapa 3x3x3 de ~5 ori mai rapidă |
| Ultima mutare a fazei unite testată întâi pe muchii | faza unită de 2–3 ori mai rapidă |
| Mutări care îndepărtează centrele, sărite când nu e loc de ele | inclus în cifra de mai sus |

Efectul pe ansamblu: aceeași lungime ca înainte (40,6) în 2,6 s în loc de 3,7 s,
iar modul full ajunge la ~40,0 fiindcă își permite de 15 ori mai multe capete.

Ce a mai rămas:

- Faza unită e în continuare ~60% din timp. Nodurile ei sunt răspândite pe
  toate nivelurile (nu doar la frunze), deci următorul câștig cere un test pe
  muchii cu două mutări înainte de capăt, nu doar cu una.
- Tabel de împerechere redus prin simetrie (240 MB → 30 MB), pentru faza 3.
- Mod fără tabelul de centre, pentru mașini cu puțină memorie.

## Moduri

- **fast** (~0,3 s): fază unită mică, fără adâncire 3x3x3. Există.
- **normal** (~4 s): implicitul de acum.
- **full** (~14 s): implicit plus căutare exactă 3x3x3. Există.
- De făcut: un mod **tiny**, fără tabelele mari, pentru mașini cu puțină memorie.

## 5x5x5: unde stau ceilalți și ce am pregătit

**Chen Shuang (cube555), citit din cod.** Cinci faze de reducere, apoi min2phase:

| Fază | Ce rezolvă | Mutări permise |
|---|---|---|
| 1 | centrele x și t de culoare R/L ajung pe fețele R/L | toate cele 36 |
| 2 | centrele U/D pe U/D, paritatea wings | 22 |
| 3 | R față de L pe fețele R/L, wings împărțite high/low, orientarea muchiilor centrale | 24 |
| 4 | patru muchii complete așezate, centre parțial | 16 |
| 5 | ultimele opt muchii și toate centrele | 14 |

- Rezultat publicat: 51–52 de mutări pentru reducere, 1–2 s, tabele de 240 MB.
  Cu 3x3x3 (~19,6) iese ~71 în total. dwalton76 raportează 78,9 doar pentru
  reducere, deci ~99 în total (corectat 2026-11-10: scrisesem „~77–79” ca
  și cum ar fi totalul).
- Unde pierde mutări, după cod:
  - etapa 3x3x3 se rezolvă o singură dată, după prima reducere găsită;
  - mutările nu se anulează la trecerea dintre faze;
  - fazele 1 și 2 sunt separate, ca la TPR înainte să le unim noi;
  - nu încearcă scramble-ul invers.
- Corectare (2026-11-10, după ce am citit `Search.java` și `PhaseSearch.java`):
  scrisesem aici că fazele lui taie lacom și că nu încearcă alte axe. Nu e
  așa. Fiecare fază pornește din 200–500 de soluții ale fazei dinainte,
  ordonate după lungimea totală, în 3 axe, iar faza 4 în două variante de
  felie. De aici vine o bună parte din rezultatul lui.

**Ce se transferă direct de la 4x4x4:**

- Lotul de reduceri comparat după lungimea totală (la 4x4x4 a adus ~2 mutări).
- Tabelul exact de centre: centrele x de la 5x5x5 se mișcă exact ca centrele
  de la 4x4x4, deci `centres-exact.bin` se refolosește ca atare. Centrele t au
  nevoie de un tabel propriu de aceeași mărime.
- Testul pe muchii la ultima mutare și căutarea pe trei axe deodată.

**Ce e diferit:**

- Centrele sunt fixe, deci cubul nu poate fi terminat în 24 de orientări; rămân
  doar cele 3 alegeri de axă.
- Două orbite de centre trebuie așezate împreună; pragul de căutare va fi
  maximul celor două tabele exacte, nu un singur tabel perfect.
- Muchiile centrale fixează orientarea, deci nu există paritatea de la 4x4x4,
  dar faza de muchii are trei piese pe muchie în loc de două.

**Făcut până acum (2026-11-10):**

- `src/cuben.rs`: model pe stickere pentru orice mărime, din geometrie.
- `src/c5.rs`: model pe piese. Un 5x5x5 fără rândurile din mijloc e un 4x4x4,
  deci modelul e cubul de 4x4x4 plus muchiile centrale plus centrele t.
- Set fix de 200 de scramble-uri și măsurătoarea cube555: 70,58 în total.
- `src/stage5.rs`: așezarea centrelor (ce fac fazele 1 și 2 la cube555).
  Faza A caută cu toate mutările, faza B are tabel exact (331 de milioane de
  stări). Rezultat: 17,1–17,6 mutări față de 18,28 la cube555.

**Moduri cerute, pentru toate mărimile:**

| Mod | Timp pe cub |
|---|---|
| fast | sub 1 s |
| normal | sub 5 s |
| full | fără limită |

Sunt implementate la 5x5x5 (`--mode`). Timpul se împarte pe faze: centre până
la 15% din buget, faza C până la 40%, P până la 70%, E până la 90%; o fază
întârziată se oprește doar după ce are destule soluții pentru următoarea.

**Solverul complet (2026-11-10), `src/edges5.rs` și `src/solver5.rs`:**

| Mod | Lungime medie | Timp mediu | Cuburi |
|---|---|---|---|
| cube555 | 70,58 | 1,1 s | 200 |
| fast | 70,16 | 0,34 s (max 0,74 s) | 200 |
| normal | 68,82 | 1,4 s (max 3,5 s) | 200 |
| full | 68,20 | 5,3 s | primele 40 |

- Prima încercare (faze C, D în doi pași, E; fiecare fază pornită din câteva
  soluții) a dat 73,0, mai slab decât cube555.
- Ce a adus rezultatul de acum:
  - faza P, ca faza 4 de la cube555, dar cu tabel exact pentru toate cele 12
    piese ale celor patru muchii (198 de milioane de stări) și tabele muchii
    contra centre; fără ele faza dura 5 s, cu ele 0,15 s;
  - fiecare fază pornește din toate soluțiile fazei dinainte și păstrează
    continuările cele mai scurte în total;
  - șase orientări pentru muchii (3 axe × 2 felii);
  - etapa 3x3x3 rezolvată pentru zeci sau sute de reduceri.
- Pe faze, modul normal față de cube555: centre 18,44 față de 18,28; muchii
  33,20 față de 33,05; 3x3x3 17,18 față de 19,23. Deci reducerea e la fel de
  lungă ca a lui; tot câștigul vine din etapa 3x3x3 aleasă din lot. Ținta de
  66 pe care o scrisesem aici nu e atinsă.
- Lățimea căutării ajută puțin: de 20 de ori mai lat aduce 0,6 mutări.

**Făcut după prima versiune completă (2026-11-10):**

- Tabelul fazei C redus prin cele 16 simetrii ale fazei: 2,77 GB → 175 MB,
  generat în 17 s. Simetriile se găsesc din tabelele de mutări, nu din
  geometrie (`src/sym.rs`). Verificat pe 20 de milioane de intrări față de
  tabelul vechi. Tabelele de 5x5x5 au acum 2,1 GB în loc de 4,7 GB, iar
  timpii au scăzut cu ~35% pe un Mac ocupat.
- Centrele într-o singură fază (`src/centres5.rs`): tabele exacte pe fiecare
  orbită (9,46 miliarde de colorări, 300 MB după simetrie; o orbită singură
  cere cel mult 11–12 mutări, în medie ~9). Căutarea exactă cu pragul
  „maximul celor două” a găsit 15 mutări în loc de 17 la cubul măsurat, dar
  în 340 s pe 8 nuclee: pragul e ~9, răspunsul ~15. Tăierea după suma celor
  două distanțe a pierdut soluțiile scurte (a găsit 17–19 în 20 s). Nu e
  folosită în niciun mod; se pornește cu `--joint`.

**Pași următori pentru 5x5x5:**

1. Centre: un prag mai bun decât maximul pe orbite. Candidat: un tabel comun
   pentru centrele unei singure axe din ambele orbite (problema fazei A),
   care ar avea 34 de miliarde de intrări după simetrie, deci doar o parte
   din el (stările apropiate de țintă) încape. Câștig posibil: până la 2
   mutări, dacă soluțiile de 15–16 rămân bune și pentru fazele de muchii.
2. Fazele P și E: soluțiile bune se găsesc la 2–4 mutări peste prag, deci
   tabele comune muchii+centre mai mari ar grăbi căutarea și ar permite un
   lot mai lat în același timp.
3. Unirea fazelor E și 3x3x3 pentru ultimele mutări, ca la 4x4x4.

## 6x6x6 și 7x7x7 (început 2026-11-10)

Ordinea stabilită cu Horia: 6 și 7 optimizate ca mutări și ca timp, apoi
NxNxN general ca baseline, apoi îmbunătățiri pe rând.

**Referința.** dwalton76, rulat pe seturile noastre din instalarea lui Horia
(`~/MultiCuber/rubiks-cube-NxNxN-solver`, cu tabelele deja descărcate).
Tabelul din README-ul lui spune „mutări până la 3x3x3”, dar cifrele sunt
totaluri: la 4x4x4 am măsurat 51,71 pe 100 de cuburi (README: 51,6), toate
soluțiile verificate. Rezultatele pe 5, 6 și 7 sunt în `bench/RESULTS.md`.

**Ce e nou la 6x6x6.** Patru orbite de centre (x interior, x exterior, două
orbite oblice în oglindă) și două orbite de wings, fără piese fixe. Tot ce
avem se refolosește prin „cuburi virtuale” (`Geometry::sub_facelets`):

- straturile 1, 3, 4, 6 formează un 4x4x4 (colțuri, wings interioare, x
  interior); mutarea lui lată e mutarea de 3 straturi;
- straturile 1, 2, 3|4, 5, 6 formează un 5x5x5 dacă cele două straturi din
  mijloc arată la fel; mutările lui sunt chiar mutările de 1 și 2 straturi.

Observația pe care se sprijină planul: sub mutările 4x4x4-ului virtual,
fiecare sfert de față (un x interior, cei doi oblici de lângă el și x-ul
exterior din colț) se mișcă rigid. Dacă fiecare sfert are o singură culoare,
toate centrele se comportă ca centrele unui 4x4x4.

**Planul pentru prima versiune:**

1. Centrele celor patru orbite pe axele lor, în patru pași: R/L pentru x
   interior și oblici (toate cele 54 de mutări), apoi R/L pentru x exterior
   cu faza A de la 5x5x5 pe cubul virtual (perechile de oblici sunt centrele
   lui t); la fel pentru U/D, cu tabelul exact al fazei B.
2. Sferturile într-o singură culoare: doar jumătăți de tură late și fețe;
   pe fiecare axă starea are 70⁴ valori, deci tabel exact pe axă.
3. Solverul nostru de 4x4x4 pe cubul virtual, fără etapa 3x3x3: rezolvă
   toate centrele și împerechează wings interioare, cu paritate cu tot.
4. Solverul nostru de 5x5x5 pe cubul virtual: centrele sunt gata, deci
   rămân fazele de muchii și 3x3x3.

Estimare: ~37 + ~13 + ~23 + ~50 ≈ 120–125 de mutări. E o estimare, nu o
măsurătoare.

**Stadiu (2026-11-10):** pasul 1 e scris și verificat (`src/stage6.rs`):
36,55 de mutări în 0,48 s, față de ~38 la dwalton76. A ieșit în patru
căutări, nu cum scrie mai sus: (a) x interior R/L pe fețe și oblicii R/L
împerecheați, toate mutările; (b) faza A de la 5x5x5 pe cubul virtual;
(c) x interior U/D, paritatea wings interioare și oblicii U/D împerecheați;
(d) faza B de la 5x5x5. Împerecherea în loc de așezare a adus 14 mutări.
Pașii 2–4 urmează.

**7x7x7** urmează aceeași schemă: 5x5x5 virtual din straturile 1, 3, 4, 5, 7
pentru interior, tripletele oblic–t–oblic în locul perechilor, apoi 5x5x5
virtual din straturile 1, 2, 4, 6, 7.

**Kociemba, RubikNxNxNSolver** (citit 2026-11-10): rezolvă doar centrele,
în metrica de felii simple, ~1,19 mutări pe centru la 101x101x101; pentru
7x7x7 ar însemna ~170 de mutări doar pe centre, deci nu ajută la 6 și 7.
Pentru solverul general e scheletul potrivit: fiecare pereche de orbite
oblice (x, y) se rezolvă împreună, doar cu fețe și feliile x și y, iar
orbita „+” a fiecărei felii e martorul că felia a fost întoarsă la loc, deci
perechile deja rezolvate nu se strică.

## Spre mărimi mai mari

- 6x6x6 și 7x7x7 au centre oblice; singura referință e dwalton76.
- Pentru N general: reducere pe orbite, cu tabele exacte doar pentru orbitele
  mici. Modelul pe stickere acoperă deja orice N.

## Despre tabelele lui dwalton76

Pentru 4x4x4 are opt tabele cu între 840 și 5,9 milioane de rânduri, ca text:
fiecare rând e o stare și secvența care o rezolvă. Sunt mari pe disc din cauza
formatului, nu a numărului de stări. Gigabiții sunt mai ales pentru 5x5x5–7x7x7.
Ale noastre țin 2 biți pe stare (distanța modulo 3) și acoperă spații de până
la 9,5 miliarde de stări; de aici vine diferența de lungime, nu din mărime.

## Riscuri

- Tabelele ocupă 4,9 GB pe disc și se generează în ~4 minute la prima rulare.
- Limita de timp e „moale": fiecare etapă își așteaptă primul rezultat.
- Rezultatele variază cu câteva sutimi între rulări, din cauza paralelismului.

## Surse

- TPR: https://github.com/cs0x7f/TPR-4x4x4-Solver
- dwalton76: https://github.com/dwalton76/rubiks-cube-NxNxN-solver
- Limita 54 pentru fazele 1+2+3+3x3x3 (neverificată): https://github.com/AnanasClassic/4x4x4-obtm-54-research
