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
  - doar faza 5 vede tot lotul de candidați (500); fazele dinainte taie lacom;
  - fazele 1 și 2 sunt separate, ca la TPR înainte să le unim noi;
  - nu încearcă scramble-ul invers și nici altă axă după faza 1.

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

**Pregătit acum:** `src/cuben.rs`, un model pe stickere pentru orice mărime,
construit doar din geometrie și verificat mutare cu mutare față de modelul de
4x4x4. Aplică scramble-uri și verifică soluții pentru 5x5x5, 6x6x6, 7x7x7.

**Pași următori pentru 5x5x5:**

1. Set fix de scramble-uri și măsurătoare cube555 pe Mac-ul tău, pe faze.
2. Model pe piese (centre x, centre t, muchii centrale, wings, colțuri) și
   tabelul exact pentru centrele t.
3. Faza unită de centre pentru ambele orbite, apoi fazele de muchii după
   cube555, cu lotul de reduceri și etapa 3x3x3 de la 4x4x4.
4. Abia apoi: unirea fazelor de muchii, unde cred că e câștigul cel mai mare.

Țintă realistă pentru prima versiune: sub 66 de mutări (cube555: ~71).
E o estimare din ce am câștigat la 4x4x4, nu o măsurătoare.

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
