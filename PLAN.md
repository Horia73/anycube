# Plan: solver 4x4x4 cu soluții mai scurte decât TPR

Actualizat: 2026-11-10. Țintă: medie cât mai aproape de 39 de mutări (OBTM).
Cifrele măsurate sunt în `bench/RESULTS.md`.

## Unde suntem

| Mod | Medie | Timp mediu | Măsurat pe |
|---|---|---|---|
| TPR (referință) | 44,57 | 0,15 s | 1.000 de cuburi |
| `--mode fast` | 41,82 | 0,34 s | primele 100 |
| implicit | 40,67 | 3,7 s | 1.000 de cuburi |
| `--mode full` | vezi `bench/RESULTS.md` | ~14 s | primele 100 |

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

Unde se duce timpul în modul implicit: faza unită ~40%, faza 3 ~25%, 3x3x3 ~35%.

| Pas | Câștig așteptat |
|---|---|
| Tabelele mici salvate pe disc (acum se refac la fiecare pornire) | pornire din ~0,5 s în ~0,05 s |
| Urmărirea muchiilor în faza unită, în loc de reaplicarea secvenței la capăt | faza unită de ~1,5 ori mai rapidă |
| Tabel de împerechere redus prin simetrie (240 MB → 30 MB, în cache) | faza 3 de ~2 ori mai rapidă |
| Mod fără tabelul de centre (fazele 1 și 2 separate) | 3 MB de tabele în loc de 4,9 GB |
| Proces rezident sau bibliotecă, pentru rezolvări în serie | fără cost de pornire |

## Moduri

- **fast** (~0,3 s): fază unită mică, fără adâncire 3x3x3. Există.
- **normal** (~4 s): implicitul de acum.
- **full** (~14 s): implicit plus căutare exactă 3x3x3. Există.
- De făcut: un mod **tiny**, fără tabelele mari, pentru mașini cu puțină memorie.

## Spre NxNxN

Ce se generalizează și ce nu:

- **Se păstrează:** etapa 3x3x3 (two-phase și exact), ideea de lot de reduceri
  comparat după lungimea totală, variantele de orientare, limita de timp.
- **Se rescrie:** modelul de cub și fazele de reducere. Acum sunt scrise de
  mână pentru 4x4x4, după numerotarea TPR.

Pași propuși:

1. **Model generic.** Piesele pe orbite (colțuri, muchii centrale, perechi de
   wings, orbite de centre), cu mutările generate din geometrie, nu din liste.
2. **5x5x5.** Are două orbite de centre de câte 24 (ca la 4x4x4), 24 de wings
   și 12 muchii centrale care fixează orientarea. Fără paritatea de la 4x4x4.
3. **6x6x6 și 7x7x7.** Apar centrele oblice; aici dwalton76 e singura referință.
4. **N general.** Reducere pe orbite, cu tabele doar pentru orbitele mici.

Avertisment: tabelul exact de centre (9,5 miliarde de stări) e posibil doar
pentru o orbită. La 5x5x5 sunt două orbite care trebuie rezolvate împreună,
deci va fi nevoie de alte faze, nu doar de tabele mai mari.

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
