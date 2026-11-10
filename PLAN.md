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
  Cu 3x3x3 (~19,6) iese ~71 în total. dwalton76 raportează ~77–79.
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
| fast | 70,17 | 0,44 s | 200 |
| normal | 68,98 | 2,2 s (max 4,9 s) | 200 |
| full | 68,20 | 8,7 s | primele 40 |

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
- Pe faze, modul normal față de cube555: centre 18,53 față de 18,28; muchii
  33,28 față de 33,05; 3x3x3 17,16 față de 19,23. Deci reducerea e la fel de
  lungă ca a lui; tot câștigul vine din etapa 3x3x3 aleasă din lot. Ținta de
  66 pe care o scrisesem aici nu e atinsă.
- Lățimea căutării ajută puțin: de 20 de ori mai lat aduce 0,6 mutări.

**Pași următori pentru 5x5x5, în ordinea câștigului estimat:**

1. Centrele într-o singură fază (x și t împreună, cu tabelele exacte pe
   orbită ca prag). Numărătoarea de stări spune ~16 mutări în loc de 18,5.
   Căutarea e grea: pragul e maximul a două tabele, nu un tabel comun.
2. Tabelul fazei C redus prin simetrie (2,77 GB → ~0,2 GB). Nu scurtează
   soluțiile, dar scade memoria la jumătate și timpul primului solve; pe un
   Mac ocupat, tabelul de acum e scos din memorie și faza C trece de la
   50 ms la 1–2 s.
3. Fazele P și E: soluțiile bune se găsesc la 2–4 mutări peste prag, deci
   tabele comune muchii+centre mai mari ar grăbi căutarea și ar permite un
   lot mai lat în același timp.
4. Unirea fazelor E și 3x3x3 pentru ultimele mutări, ca la 4x4x4.

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
